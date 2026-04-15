// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Query, State};
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

use crate::state::AppState;

/// Accepts authentication via:
/// - Authorization: Bearer <session_token>
/// - X-API-Key: <api_key>
/// - ?key=<api_key> query parameter
/// - ?token=<session_token> query parameter
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check session token from Authorization header
    if let Some(token) = extract_bearer_token(&req) {
        if validate_session(&state.db, &token).await {
            return Ok(next.run(req).await);
        }
    }

    // Check session token from query param
    if let Some(token) = params.get("token") {
        if validate_session(&state.db, token).await {
            return Ok(next.run(req).await);
        }
    }

    // Check API key from header
    if let Some(key) = req
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
    {
        if validate_api_key(&state.db, key).await {
            return Ok(next.run(req).await);
        }
    }

    // Check API key from query param
    if let Some(key) = params.get("key") {
        if validate_api_key(&state.db, key).await {
            return Ok(next.run(req).await);
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

fn extract_bearer_token(req: &Request<axum::body::Body>) -> Option<String> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

async fn validate_session(db: &SqlitePool, token: &str) -> bool {
    sqlx::query_scalar::<_, i32>(
        "SELECT COUNT(*) FROM sessions WHERE token = ? AND expires_at > datetime('now')",
    )
    .bind(token)
    .fetch_one(db)
    .await
    .unwrap_or(0)
        > 0
}

async fn validate_api_key(db: &SqlitePool, key: &str) -> bool {
    let hash = hash_key(key);
    sqlx::query_scalar::<_, i32>("SELECT COUNT(*) FROM api_keys WHERE key_hash = ?")
        .bind(&hash)
        .fetch_one(db)
        .await
        .unwrap_or(0)
        > 0
}

pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, 12)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

pub fn generate_session_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..48)
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

pub fn generate_api_key() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let chars: String = (0..32)
        .map(|_| {
            let idx = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();
    format!("mxs_{}", chars)
}
