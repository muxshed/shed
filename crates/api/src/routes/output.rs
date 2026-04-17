// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            video_bitrate_kbps: 4500,
            audio_bitrate_kbps: 160,
            width: 1920,
            height: 1080,
            fps: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputStats {
    pub bytes_sent: u64,
    pub duration_secs: f64,
    pub source_bitrate_kbps: f64,
    pub output_bitrate_kbps: u32,
    pub dropped_frames: u64,
    pub source_width: Option<u32>,
    pub source_height: Option<u32>,
    pub source_fps: Option<f64>,
    pub source_encoder: Option<String>,
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OutputConfig>, ApiError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings WHERE key = 'output_config'")
            .fetch_optional(&state.db)
            .await?;

    let config = match row {
        Some((json,)) => serde_json::from_str(&json).unwrap_or_default(),
        None => OutputConfig::default(),
    };

    Ok(Json(config))
}

pub async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<OutputConfig>,
) -> Result<Json<OutputConfig>, ApiError> {
    let json = serde_json::to_string(&config)
        .map_err(|e| muxshed_common::MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('output_config', ?)")
        .bind(&json)
        .execute(&state.db)
        .await?;

    Ok(Json(config))
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OutputStats>, ApiError> {
    let mut stats = state.egress.stats().await;

    // Get configured output bitrate
    let out_config: OutputConfig =
        sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|(json,)| serde_json::from_str(&json).ok())
            .unwrap_or_default();
    stats.output_bitrate_kbps = out_config.video_bitrate_kbps + out_config.audio_bitrate_kbps;

    // Get current program source's media info
    let program_id = *state.program_source.borrow();
    if let Some(id) = program_id {
        let infos = state.source_media_info.read().await;
        if let Some(info) = infos.get(&id) {
            stats.source_width = info.width;
            stats.source_height = info.height;
            stats.source_fps = info.fps;
            stats.source_encoder = info.encoder.clone();
        }
    }

    Ok(Json(stats))
}
