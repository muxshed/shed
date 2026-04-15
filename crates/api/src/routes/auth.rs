// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{generate_session_token, verify_password};
use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::MuxshedError;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub role: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
pub struct LogoutRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, role FROM users WHERE username = ?",
    )
    .bind(&body.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::Unauthorized("invalid credentials".to_string()))?;

    if !verify_password(&body.password, &user.password_hash) {
        return Err(MuxshedError::Unauthorized("invalid credentials".to_string()).into());
    }

    let token = generate_session_token();
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    sqlx::query(
        "INSERT INTO sessions (id, user_id, token, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&user.id)
    .bind(&token)
    .bind(&expires_at)
    .execute(&state.db)
    .await?;

    // Update last_used tracking could go here

    Ok(Json(LoginResponse {
        token,
        username: user.username,
        role: user.role,
        expires_at,
    }))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LogoutRequest>,
) -> Result<StatusCode, ApiError> {
    sqlx::query("DELETE FROM sessions WHERE token = ?")
        .bind(&body.token)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::OK)
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Json<MeResponse>, ApiError> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| MuxshedError::Unauthorized("no session token".to_string()))?;

    let session = sqlx::query_as::<_, SessionRow>(
        "SELECT s.user_id, u.username, u.role FROM sessions s JOIN users u ON s.user_id = u.id WHERE s.token = ? AND s.expires_at > datetime('now')",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::Unauthorized("invalid or expired session".to_string()))?;

    Ok(Json(MeResponse {
        id: session.user_id,
        username: session.username,
        role: session.role,
    }))
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: String,
    username: String,
    role: String,
}
