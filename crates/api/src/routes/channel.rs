// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::auth::hash_password;
use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{ChannelConfig, ChannelInfo, MuxshedError};

#[derive(sqlx::FromRow)]
struct ChannelRow {
    enabled: i64,
    token: String,
    title: String,
    logo_path: Option<String>,
    accent: Option<String>,
    password_hash: Option<String>,
}

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..16u8);
            std::char::from_digit(idx as u32, 16).unwrap()
        })
        .collect()
}

fn row_to_config(row: &ChannelRow) -> ChannelConfig {
    ChannelConfig {
        enabled: row.enabled != 0,
        token: row.token.clone(),
        title: row.title.clone(),
        logo_url: row
            .logo_path
            .as_ref()
            .map(|_| format!("/api/v1/public/channel/{}/logo", row.token)),
        accent: row.accent.clone(),
        password_protected: row.password_hash.is_some(),
        watch_path: format!("/watch/{}", row.token),
    }
}

/// Load the singleton channel row, creating it with a random token if missing.
async fn load_or_create(state: &AppState) -> Result<ChannelRow, ApiError> {
    if let Some(row) =
        sqlx::query_as::<_, ChannelRow>("SELECT enabled, token, title, logo_path, accent, password_hash FROM channel WHERE id = 1")
            .fetch_optional(&state.db)
            .await?
    {
        return Ok(row);
    }

    let token = generate_token();
    sqlx::query("INSERT OR IGNORE INTO channel (id, token) VALUES (1, ?)")
        .bind(&token)
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, ChannelRow>(
        "SELECT enabled, token, title, logo_path, accent, password_hash FROM channel WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(row)
}

// ── Authenticated handlers ───────────────────────────────────────────────────

pub async fn get_channel(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ChannelConfig>, ApiError> {
    let row = load_or_create(&state).await?;
    Ok(Json(row_to_config(&row)))
}

#[derive(Deserialize)]
pub struct UpdateChannel {
    pub enabled: Option<bool>,
    pub title: Option<String>,
    pub accent: Option<Option<String>>,
}

pub async fn update_channel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateChannel>,
) -> Result<Json<ChannelConfig>, ApiError> {
    let existing = load_or_create(&state).await?;

    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    let title = body.title.unwrap_or(existing.title.clone());
    let accent = match body.accent {
        Some(a) => a,
        None => existing.accent.clone(),
    };

    sqlx::query("UPDATE channel SET enabled = ?, title = ?, accent = ? WHERE id = 1")
        .bind(enabled as i64)
        .bind(&title)
        .bind(&accent)
        .execute(&state.db)
        .await?;

    // Start/stop the HLS output to match the new enabled state (if currently live).
    if body.enabled.is_some() {
        super::stream::ensure_channel_hls(&state).await;
    }

    let row = sqlx::query_as::<_, ChannelRow>(
        "SELECT enabled, token, title, logo_path, accent, password_hash FROM channel WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row_to_config(&row)))
}

pub async fn regenerate_token(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ChannelConfig>, ApiError> {
    load_or_create(&state).await?;
    let token = generate_token();

    sqlx::query("UPDATE channel SET token = ? WHERE id = 1")
        .bind(&token)
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, ChannelRow>(
        "SELECT enabled, token, title, logo_path, accent, password_hash FROM channel WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    // Re-point the HLS output at the new token (if currently live and enabled).
    super::stream::ensure_channel_hls(&state).await;

    Ok(Json(row_to_config(&row)))
}

pub async fn upload_logo(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_or_create(&state).await?;

    let mut file_data: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MuxshedError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("logo").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| MuxshedError::BadRequest(e.to_string()))?;
            file_data = Some((filename, data.to_vec()));
        }
    }

    let (filename, data) =
        file_data.ok_or_else(|| MuxshedError::BadRequest("no file provided".to_string()))?;

    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let ext: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(5)
        .collect();
    let ext = if ext.is_empty() { "png".to_string() } else { ext };

    let config = state.config.read().await;
    let channel_dir = config.data_dir.join("channel");
    drop(config);

    tokio::fs::create_dir_all(&channel_dir)
        .await
        .map_err(|e| MuxshedError::Internal(format!("create dir: {}", e)))?;

    // Remove any previously stored logo with a different extension.
    if let Some(old) = &row.logo_path {
        if old != &channel_dir.join(format!("logo.{}", ext)).display().to_string() {
            let _ = tokio::fs::remove_file(old).await;
        }
    }

    let file_path = channel_dir.join(format!("logo.{}", ext));
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| MuxshedError::Internal(format!("create file: {}", e)))?;
    file.write_all(&data)
        .await
        .map_err(|e| MuxshedError::Internal(format!("write file: {}", e)))?;

    let file_path_str = file_path.display().to_string();
    sqlx::query("UPDATE channel SET logo_path = ? WHERE id = 1")
        .bind(&file_path_str)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "logo_url": format!("/api/v1/public/channel/{}/logo", row.token)
    })))
}

pub async fn delete_logo(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    let row = load_or_create(&state).await?;

    if let Some(path) = &row.logo_path {
        let _ = tokio::fs::remove_file(path).await;
    }

    sqlx::query("UPDATE channel SET logo_path = NULL WHERE id = 1")
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetPassword {
    pub password: Option<String>,
}

pub async fn set_password(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetPassword>,
) -> Result<StatusCode, ApiError> {
    load_or_create(&state).await?;

    match body.password {
        Some(p) if !p.is_empty() => {
            let hash = hash_password(&p)
                .map_err(|e| MuxshedError::Internal(format!("password hash failed: {}", e)))?;
            sqlx::query("UPDATE channel SET password_hash = ? WHERE id = 1")
                .bind(&hash)
                .execute(&state.db)
                .await?;
        }
        _ => {
            sqlx::query("UPDATE channel SET password_hash = NULL WHERE id = 1")
                .execute(&state.db)
                .await?;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Public handlers ──────────────────────────────────────────────────────────

async fn fetch_by_token(state: &AppState, token: &str) -> Result<Option<ChannelRow>, ApiError> {
    let row = sqlx::query_as::<_, ChannelRow>(
        "SELECT enabled, token, title, logo_path, accent, password_hash FROM channel WHERE token = ?",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?;
    Ok(row)
}

/// Cookie value gating segment/playlist access for a password-protected channel.
fn watch_cookie_value(password_hash: &str, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}{}", password_hash, token).as_bytes());
    hex::encode(hasher.finalize())
}

fn cookie_present(headers: &HeaderMap, name: &str, expected: &str) -> bool {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|cookies| {
            cookies.split(';').any(|c| {
                let c = c.trim();
                c.split_once('=')
                    .map(|(k, v)| k == name && v == expected)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

async fn hls_live(state: &AppState, token: &str) -> bool {
    let config = state.config.read().await;
    let path = config.data_dir.join("hls").join(token).join("index.m3u8");
    drop(config);
    tokio::fs::metadata(&path).await.is_ok()
}

pub async fn public_info(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<ChannelInfo>, ApiError> {
    let row = fetch_by_token(&state, &token)
        .await?
        .ok_or_else(|| MuxshedError::NotFound("channel".to_string()))?;

    let live = hls_live(&state, &row.token).await;

    Ok(Json(ChannelInfo {
        title: row.title.clone(),
        logo_url: row
            .logo_path
            .as_ref()
            .map(|_| format!("/api/v1/public/channel/{}/logo", row.token)),
        accent: row.accent.clone(),
        live,
        requires_password: row.password_hash.is_some(),
    }))
}

#[derive(Deserialize)]
pub struct Unlock {
    pub password: String,
}

pub async fn unlock(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    Json(body): Json<Unlock>,
) -> Response {
    let row = match fetch_by_token(&state, &token).await {
        Ok(Some(row)) => row,
        Ok(None) => return status(StatusCode::NOT_FOUND),
        Err(e) => return e.into_response(),
    };

    let Some(hash) = row.password_hash.as_ref() else {
        // No password set — nothing to unlock.
        return status(StatusCode::OK);
    };

    if !crate::auth::verify_password(&body.password, hash) {
        return status(StatusCode::UNAUTHORIZED);
    }

    let value = watch_cookie_value(hash, &row.token);
    let cookie = format!(
        "mxs_watch_{}={}; Path=/; HttpOnly; SameSite=Lax",
        row.token, value
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::SET_COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

/// Returns Some(()) if access is allowed, or an error Response if the cookie gate fails.
fn check_cookie(row: &ChannelRow, headers: &HeaderMap) -> Result<(), Response> {
    if let Some(hash) = row.password_hash.as_ref() {
        let cookie_name = format!("mxs_watch_{}", row.token);
        let expected = watch_cookie_value(hash, &row.token);
        if !cookie_present(headers, &cookie_name, &expected) {
            return Err(status(StatusCode::FORBIDDEN));
        }
    }
    Ok(())
}

pub async fn serve_playlist(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let row = match fetch_by_token(&state, &token).await {
        Ok(Some(row)) => row,
        Ok(None) => return status(StatusCode::NOT_FOUND),
        Err(e) => return e.into_response(),
    };

    if let Err(resp) = check_cookie(&row, &headers) {
        return resp;
    }

    let config = state.config.read().await;
    let path = config.data_dir.join("hls").join(&row.token).join("index.m3u8");
    drop(config);

    match tokio::fs::read(&path).await {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

pub async fn serve_segment(
    State(state): State<Arc<AppState>>,
    Path((token, segment)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let row = match fetch_by_token(&state, &token).await {
        Ok(Some(row)) => row,
        Ok(None) => return status(StatusCode::NOT_FOUND),
        Err(e) => return e.into_response(),
    };

    if let Err(resp) = check_cookie(&row, &headers) {
        return resp;
    }

    // Validate filename: no path traversal, restricted charset, allowed extensions.
    let valid_name = !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    let valid_ext = segment.ends_with(".ts") || segment.ends_with(".m4s");
    if !valid_name || !valid_ext {
        return status(StatusCode::NOT_FOUND);
    }

    let config = state.config.read().await;
    let path = config.data_dir.join("hls").join(&row.token).join(&segment);
    drop(config);

    match tokio::fs::read(&path).await {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "video/mp2t")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

pub async fn serve_logo(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Response {
    let row = match fetch_by_token(&state, &token).await {
        Ok(Some(row)) => row,
        Ok(None) => return status(StatusCode::NOT_FOUND),
        Err(e) => return e.into_response(),
    };

    let Some(path) = row.logo_path.as_ref() else {
        return status(StatusCode::NOT_FOUND);
    };

    let content_type = match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    match tokio::fs::read(path).await {
        Ok(data) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

fn status(code: StatusCode) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}
