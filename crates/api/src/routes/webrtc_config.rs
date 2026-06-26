// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! ICE configuration for guest WebRTC. Stored as a `settings` blob and shared by
//! both the server-side peer (`crate::guest_webrtc`) and the guest browser (via
//! the public `GET /guest/{token}` response), so the two always agree on which
//! STUN/TURN servers to use. Off-LAN guests behind strict NAT need a TURN entry
//! here (see `docs/guests-webrtc.md`).

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;

/// A single ICE server, shaped to match the browser `RTCIceServer` dictionary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebrtcConfig {
    pub ice_servers: Vec<IceServer>,
}

impl Default for WebrtcConfig {
    fn default() -> Self {
        Self {
            ice_servers: vec![IceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                username: None,
                credential: None,
            }],
        }
    }
}

/// Load the configured ICE servers, falling back to the STUN default.
pub async fn load(state: &AppState) -> WebrtcConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'webrtc_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(json,)| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<WebrtcConfig>, ApiError> {
    Ok(Json(load(&state).await))
}

pub async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<WebrtcConfig>,
) -> Result<Json<WebrtcConfig>, ApiError> {
    let json = serde_json::to_string(&config)
        .map_err(|e| muxshed_common::MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('webrtc_config', ?)")
        .bind(&json)
        .execute(&state.db)
        .await?;

    Ok(Json(config))
}
