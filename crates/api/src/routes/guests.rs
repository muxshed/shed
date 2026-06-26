// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::MuxshedError;

#[derive(Deserialize)]
pub struct CreateGuest {
    pub name: String,
}

#[derive(Serialize)]
pub struct GuestResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub url: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct GuestListItem {
    pub id: String,
    pub name: String,
    pub token: String,
    pub status: String,
    pub created_at: String,
}

/// Public, token-scoped info for the guest join page (no auth).
#[derive(Serialize)]
pub struct GuestInfo {
    pub name: String,
    pub status: String,
}

// ── Authenticated (operator) ──────────────────────────────────────────────────

pub async fn invite(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGuest>,
) -> Result<(StatusCode, Json<GuestResponse>), ApiError> {
    if body.name.trim().is_empty() {
        return Err(MuxshedError::BadRequest("guest name is required".to_string()).into());
    }
    let id = Uuid::new_v4();
    let token = generate_token();

    sqlx::query("INSERT INTO guests (id, name, token) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(body.name.trim())
        .bind(&token)
        .execute(&state.db)
        .await?;

    let row = fetch_row(&state, &id.to_string()).await?;
    Ok((
        StatusCode::CREATED,
        Json(GuestResponse {
            url: format!("/guest/{}", row.token),
            id: row.id,
            name: row.name,
            token: row.token,
            status: row.status,
            created_at: row.created_at,
        }),
    ))
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<GuestListItem>>, ApiError> {
    let rows = sqlx::query_as::<_, GuestRow>(
        "SELECT id, name, token, status, created_at FROM guests ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| GuestListItem {
                id: r.id,
                name: r.name,
                token: r.token,
                status: r.status,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM guests WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("guest {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Public (guest, by token) ──────────────────────────────────────────────────

/// Validate a guest link and return its name + status for the join page.
pub async fn public_info(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<GuestInfo>, ApiError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT name, status FROM guests WHERE token = ?",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound("guest link not found".to_string()))?;

    Ok(Json(GuestInfo {
        name: row.0,
        status: row.1,
    }))
}

/// WHIP endpoint — a guest publishes their camera/mic here over WebRTC.
///
/// Creates an ephemeral `web_rtc` Source for the guest, terminates the peer, and
/// bridges the guest's RTP into the media relay via FFmpeg (see
/// `crate::guest_webrtc`). Returns the SDP answer per the WHIP spec. The live
/// media path is verifiable only against a real browser; see `docs/guests-webrtc.md`.
pub async fn whip(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    offer: String,
) -> Response {
    let guest = sqlx::query_as::<_, (String,)>("SELECT name FROM guests WHERE token = ?")
        .bind(&token)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    let Some((name,)) = guest else {
        return (StatusCode::NOT_FOUND, "guest link not found").into_response();
    };

    // Ephemeral source row so the guest appears in the switcher while connected.
    let source_id = Uuid::new_v4();
    let kind = muxshed_common::SourceKind::WebRtc {
        token: token.clone(),
    };
    let kind_json = match serde_json::to_string(&kind) {
        Ok(j) => j,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    if sqlx::query("INSERT INTO sources (id, name, kind) VALUES (?, ?, ?)")
        .bind(source_id.to_string())
        .bind(format!("{} (guest)", name))
        .bind(&kind_json)
        .execute(&state.db)
        .await
        .is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not create source").into_response();
    }

    let _ = sqlx::query("UPDATE guests SET status = 'connecting', source_id = ? WHERE token = ?")
        .bind(source_id.to_string())
        .bind(&token)
        .execute(&state.db)
        .await;

    match crate::guest_webrtc::start_guest_ingest(state.clone(), source_id, offer).await {
        Ok(answer) => Response::builder()
            .status(StatusCode::CREATED)
            .header(header::CONTENT_TYPE, "application/sdp")
            .header(header::LOCATION, format!("/api/v1/guest/{}/whip", token))
            .body(axum::body::Body::from(answer))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::warn!("guest ingest failed for {}: {}", source_id, e);
            crate::guest_webrtc::stop_guest_ingest(&state, &source_id).await;
            (StatusCode::BAD_REQUEST, format!("could not start ingest: {}", e)).into_response()
        }
    }
}

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..36u8);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

async fn fetch_row(state: &AppState, id: &str) -> Result<GuestRow, ApiError> {
    let row = sqlx::query_as::<_, GuestRow>(
        "SELECT id, name, token, status, created_at FROM guests WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(row)
}

#[derive(sqlx::FromRow)]
struct GuestRow {
    id: String,
    name: String,
    token: String,
    status: String,
    created_at: String,
}
