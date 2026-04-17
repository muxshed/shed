// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
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
    pub created_at: String,
}

#[derive(Serialize)]
pub struct GuestListItem {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

pub async fn invite(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGuest>,
) -> Result<(StatusCode, Json<GuestResponse>), ApiError> {
    let id = Uuid::new_v4();
    let token = generate_token();

    sqlx::query("INSERT INTO guests (id, name, token) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(&body.name)
        .bind(&token)
        .execute(&state.db)
        .await?;

    let url = format!("/guest?token={}", token);

    let row = sqlx::query_as::<_, GuestRow>("SELECT id, name, token, created_at FROM guests WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(GuestResponse {
            id: row.id,
            name: row.name,
            token: row.token,
            url,
            created_at: row.created_at,
        }),
    ))
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<GuestListItem>>, ApiError> {
    let rows = sqlx::query_as::<_, GuestRow>("SELECT id, name, token, created_at FROM guests")
        .fetch_all(&state.db)
        .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| GuestListItem {
                id: r.id,
                name: r.name,
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

#[derive(sqlx::FromRow)]
struct GuestRow {
    id: String,
    name: String,
    token: String,
    created_at: String,
}
