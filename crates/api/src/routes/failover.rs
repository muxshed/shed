// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{FailoverConfig, MuxshedError};

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<FailoverConfig>, ApiError> {
    Ok(Json(crate::failover::load_config(&state).await))
}

pub async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(cfg): Json<FailoverConfig>,
) -> Result<Json<FailoverConfig>, ApiError> {
    let json = serde_json::to_string(&cfg).map_err(|e| MuxshedError::Internal(e.to_string()))?;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('failover_config', ?)")
        .bind(&json)
        .execute(&state.db)
        .await?;
    // Nudge the supervisor to re-read config immediately.
    let current = *state.program_intent.borrow();
    let _ = state.program_intent.send(current);
    Ok(Json(cfg))
}
