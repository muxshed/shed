// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! WHIP ingest endpoint (RFC 9725 style). A publisher (OBS/hardware over H.264,
//! or a browser over VP8) POSTs an SDP offer with `Authorization: Bearer <token>`
//! where the token is the source's `web_rtc` token. The token lives only in the
//! header, never the URL, so the endpoint is the same for every WHIP source.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;
use crate::webrtc_ingest::{self, IngestKind};

/// Extract the bearer token from an `Authorization: Bearer <token>` header.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let rest = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    let t = rest.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Resolve the persistent WHIP source id for a bearer token. Matches a source
/// whose kind JSON is a `web_rtc` with this token. Returns 401 on any mismatch.
async fn resolve_source(state: &AppState, token: &str) -> Result<Uuid, StatusCode> {
    // Match the exact `web_rtc` token embedded in the kind JSON.
    let needle = format!("%\"token\":\"{}\"%", token);
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, kind FROM sources WHERE kind LIKE ?")
            .bind(&needle)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let (id, kind_json) = row.ok_or(StatusCode::UNAUTHORIZED)?;
    // Confirm it is really a web_rtc source with this exact token (guard against
    // a LIKE match inside another field).
    let kind: muxshed_common::SourceKind =
        serde_json::from_str(&kind_json).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let token_matches = matches!(
        &kind,
        muxshed_common::SourceKind::WebRtc { token: t } if t == token
    );
    if !token_matches {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Refuse guest ephemeral sources: /whip is only for persistent WHIP sources.
    // A guest publishes through /guest/{token}/whip and its source row is tied to
    // a `guests` record; accepting it here and tearing it down as a persistent
    // source would leak the guest's row and status. Guests are excluded by their
    // link to the guests table.
    let is_guest: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM guests WHERE source_id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if is_guest.is_some() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Uuid::parse_str(&id).map_err(|_| StatusCode::UNAUTHORIZED)
}

/// `POST /api/v1/whip` — publish. Body is the SDP offer.
#[utoipa::path(
    post,
    path = "/api/v1/whip",
    tag = "sources",
    request_body(content = String, description = "SDP offer", content_type = "application/sdp"),
    responses(
        (status = 201, description = "SDP answer", content_type = "application/sdp"),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 409, description = "A publisher is already live for this source")
    )
)]
pub async fn publish(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    offer: String,
) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let source_id = match resolve_source(&state, &token).await {
        Ok(id) => id,
        Err(code) => return (code, "invalid bearer token").into_response(),
    };

    // Single-publisher: reject if this source already has a live/connecting peer.
    {
        let states = state.source_states.read().await;
        if matches!(
            states.get(&source_id),
            Some(muxshed_common::SourceState::Live)
                | Some(muxshed_common::SourceState::Connecting)
        ) {
            return (StatusCode::CONFLICT, "source already publishing").into_response();
        }
    }
    if state.guest_peers.read().await.contains_key(&source_id) {
        return (StatusCode::CONFLICT, "source already publishing").into_response();
    }

    match webrtc_ingest::start_ingest(state.clone(), source_id, offer, IngestKind::Persistent).await
    {
        Ok(answer) => {
            let session = Uuid::new_v4();
            {
                let mut sessions = state.whip_sessions.write().await;
                sessions.retain(|_, sid| *sid != source_id);
                sessions.insert(session, source_id);
            }
            Response::builder()
                .status(StatusCode::CREATED)
                .header(header::CONTENT_TYPE, "application/sdp")
                .header(header::LOCATION, format!("/api/v1/whip/{}", session))
                .body(axum::body::Body::from(answer))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            tracing::warn!("whip publish failed for {}: {}", source_id, e);
            webrtc_ingest::stop_ingest(&state, &source_id, IngestKind::Persistent).await;
            (StatusCode::BAD_REQUEST, format!("could not start ingest: {}", e)).into_response()
        }
    }
}

/// `DELETE /api/v1/whip/{session}` — teardown the publish session.
#[utoipa::path(
    delete,
    path = "/api/v1/whip/{session}",
    tag = "sources",
    params(("session" = String, Path, description = "WHIP session id from the Location header")),
    responses((status = 204, description = "Session torn down"))
)]
pub async fn teardown(
    State(state): State<Arc<AppState>>,
    Path(session): Path<String>,
) -> Response {
    let Ok(session_id) = Uuid::parse_str(&session) else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let source_id = state.whip_sessions.write().await.remove(&session_id);
    if let Some(source_id) = source_id {
        webrtc_ingest::stop_ingest(&state, &source_id, IngestKind::Persistent).await;
    }
    StatusCode::NO_CONTENT.into_response()
}
