// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{generate_api_key, hash_key, hash_password};
use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::MuxshedError;

#[derive(Serialize)]
pub struct SetupStatus {
    pub needs_setup: bool,
}

#[derive(Deserialize)]
pub struct InitSetup {
    pub instance_name: Option<String>,
}

#[derive(Serialize)]
pub struct InitResponse {
    pub username: String,
    pub api_key: String,
    pub message: String,
}

pub async fn status(State(state): State<Arc<AppState>>) -> Result<Json<SetupStatus>, ApiError> {
    let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    Ok(Json(SetupStatus {
        needs_setup: count == 0,
    }))
}

pub async fn init(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InitSetup>,
) -> Result<(StatusCode, Json<InitResponse>), ApiError> {
    let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    if count > 0 {
        return Err(MuxshedError::BadRequest(
            "setup already completed".to_string(),
        )
        .into());
    }

    // Create admin user with default password
    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash = hash_password("admin")
        .map_err(|e| MuxshedError::Internal(format!("password hash failed: {}", e)))?;

    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(&user_id)
        .bind("admin")
        .bind(&password_hash)
        .bind("admin")
        .execute(&state.db)
        .await?;

    // Create default API key
    let key = generate_api_key();
    let key_hash = hash_key(&key);
    let key_id = uuid::Uuid::new_v4().to_string();
    let scopes = serde_json::json!(["read", "control", "admin"]).to_string();

    sqlx::query("INSERT INTO api_keys (id, name, key_hash, scopes) VALUES (?, ?, ?, ?)")
        .bind(&key_id)
        .bind("admin")
        .bind(&key_hash)
        .bind(&scopes)
        .execute(&state.db)
        .await?;

    if let Some(name) = body.instance_name {
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('instance_name', ?)")
            .bind(&name)
            .execute(&state.db)
            .await?;
    }

    tracing::info!("setup complete — admin user and API key created");

    Ok((
        StatusCode::CREATED,
        Json(InitResponse {
            username: "admin".to_string(),
            api_key: key,
            message: "Setup complete. Default login is admin/admin — change your password after first login. Save the API key for external integrations.".to_string(),
        }),
    ))
}
