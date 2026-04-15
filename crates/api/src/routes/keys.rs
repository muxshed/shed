// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::{generate_api_key, hash_key};
use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::MuxshedError;

#[derive(Deserialize)]
pub struct CreateKey {
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Serialize)]
pub struct KeyResponse {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Serialize)]
pub struct CreateKeyResponse {
    pub id: String,
    pub name: String,
    pub key: String,
    pub scopes: Vec<String>,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<KeyResponse>>, ApiError> {
    let rows = sqlx::query_as::<_, KeyRow>(
        "SELECT id, name, scopes, created_at, last_used_at FROM api_keys",
    )
    .fetch_all(&state.db)
    .await?;

    let keys = rows
        .into_iter()
        .map(|r| KeyResponse {
            id: r.id,
            name: r.name,
            scopes: serde_json::from_str(&r.scopes).unwrap_or_default(),
            created_at: r.created_at,
            last_used_at: r.last_used_at,
        })
        .collect();

    Ok(Json(keys))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateKey>,
) -> Result<(StatusCode, Json<CreateKeyResponse>), ApiError> {
    let id = Uuid::new_v4().to_string();
    let key = generate_api_key();
    let hash = hash_key(&key);
    let scopes_json = serde_json::to_string(&body.scopes)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT INTO api_keys (id, name, key_hash, scopes) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&body.name)
        .bind(&hash)
        .bind(&scopes_json)
        .execute(&state.db)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateKeyResponse {
            id,
            name: body.name,
            key,
            scopes: body.scopes,
        }),
    ))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("api key {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(sqlx::FromRow)]
struct KeyRow {
    id: String,
    name: String,
    scopes: String,
    created_at: String,
    last_used_at: Option<String>,
}
