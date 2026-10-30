// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use std::sync::Arc;
use axum::{extract::State, Json};
use muxshed_common::MuxshedError;
use crate::error::ApiError;
use crate::state::AppState;
use crate::reliability::{WatchdogConfig, ReliabilityStatus};

pub async fn status(State(state): State<Arc<AppState>>) -> Json<ReliabilityStatus> {
    Json(crate::reliability::snapshot(&state).await)
}

pub async fn get_watchdog(State(state): State<Arc<AppState>>) -> Json<WatchdogConfig> {
    Json(crate::reliability::load_config(&state.db).await)
}

pub async fn put_watchdog(
    State(state): State<Arc<AppState>>,
    Json(mut config): Json<WatchdogConfig>,
) -> Result<Json<WatchdogConfig>, ApiError> {
    config.stale_after_seconds = config.stale_after_seconds.clamp(15, 300);
    config.cooldown_seconds = config.cooldown_seconds.clamp(90, 3600);
    let json = serde_json::to_string(&config)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind("reliability_watchdog_config").bind(json)
        .execute(&state.db).await?;
    Ok(Json(config))
}
