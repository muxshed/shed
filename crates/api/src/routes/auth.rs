// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{generate_session_token, hash_password, verify_password};
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

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<StatusCode, ApiError> {
    let (parts, body) = req.into_parts();
    let token = parts
        .headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| MuxshedError::Unauthorized("no session".to_string()))?;

    let session = sqlx::query_as::<_, SessionRow>(
        "SELECT s.user_id, u.username, u.role FROM sessions s JOIN users u ON s.user_id = u.id WHERE s.token = ? AND s.expires_at > datetime('now')",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::Unauthorized("invalid session".to_string()))?;

    let bytes = axum::body::to_bytes(axum::body::Body::new(body), 1024 * 16)
        .await
        .map_err(|_| MuxshedError::BadRequest("invalid body".to_string()))?;
    let body: ChangePasswordRequest = serde_json::from_slice(&bytes)
        .map_err(|_| MuxshedError::BadRequest("invalid json".to_string()))?;

    if body.new_password.len() < 6 {
        return Err(MuxshedError::BadRequest("password must be at least 6 characters".to_string()).into());
    }

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, role FROM users WHERE id = ?",
    )
    .bind(&session.user_id)
    .fetch_one(&state.db)
    .await?;

    if !verify_password(&body.current_password, &user.password_hash) {
        return Err(MuxshedError::BadRequest("current password is incorrect".to_string()).into());
    }

    let new_hash = hash_password(&body.new_password)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(&new_hash)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
}

pub async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<UserResponse>>, ApiError> {
    let rows = sqlx::query_as::<_, UserListRow>(
        "SELECT id, username, role, created_at FROM users ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;

    let users = rows
        .into_iter()
        .map(|r| UserResponse {
            id: r.id,
            username: r.username,
            role: r.role,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(users))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    if body.username.trim().is_empty() {
        return Err(MuxshedError::BadRequest("username required".to_string()).into());
    }
    if body.password.len() < 6 {
        return Err(MuxshedError::BadRequest("password must be at least 6 characters".to_string()).into());
    }
    if !matches!(body.role.as_str(), "admin" | "write" | "read") {
        return Err(MuxshedError::BadRequest("role must be admin, write, or read".to_string()).into());
    }

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ?")
            .bind(&body.username)
            .fetch_optional(&state.db)
            .await?;
    if existing.is_some() {
        return Err(MuxshedError::BadRequest("username already exists".to_string()).into());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let password_hash = hash_password(&body.password)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&body.username)
        .bind(&password_hash)
        .bind(&body.role)
        .execute(&state.db)
        .await?;

    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id,
            username: body.username,
            role: body.role,
            created_at,
        }),
    ))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let existing = sqlx::query_as::<_, UserListRow>(
        "SELECT id, username, role, created_at FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("user {}", user_id)))?;

    let username = body.username.unwrap_or(existing.username);
    let role = body.role.unwrap_or(existing.role);

    if !matches!(role.as_str(), "admin" | "write" | "read") {
        return Err(MuxshedError::BadRequest("role must be admin, write, or read".to_string()).into());
    }

    if let Some(ref pw) = body.password {
        if pw.len() < 6 {
            return Err(MuxshedError::BadRequest("password must be at least 6 characters".to_string()).into());
        }
        let hash = hash_password(pw)
                .map_err(|e| MuxshedError::Internal(e.to_string()))?;
        sqlx::query("UPDATE users SET username = ?, password_hash = ?, role = ? WHERE id = ?")
            .bind(&username)
            .bind(&hash)
            .bind(&role)
            .bind(&user_id)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query("UPDATE users SET username = ?, role = ? WHERE id = ?")
            .bind(&username)
            .bind(&role)
            .bind(&user_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(UserResponse {
        id: user_id,
        username,
        role,
        created_at: existing.created_at,
    }))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    let admin_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(&state.db)
            .await?;

    let user = sqlx::query_as::<_, (String,)>("SELECT role FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("user {}", user_id)))?;

    if user.0 == "admin" && admin_count <= 1 {
        return Err(MuxshedError::BadRequest("cannot delete the last admin user".to_string()).into());
    }

    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    #[allow(dead_code)]
    role: String,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: String,
    username: String,
    role: String,
}

#[derive(sqlx::FromRow)]
struct UserListRow {
    id: String,
    username: String,
    role: String,
    created_at: String,
}
