// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{FailoverConfig, MuxshedError};

/// Get the program failover config.
#[utoipa::path(
    get,
    path = "/api/v1/failover",
    tag = "failover",
    responses((status = 200, description = "Failover config", body = muxshed_common::FailoverConfig))
)]
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<FailoverConfig>, ApiError> {
    Ok(Json(crate::failover::load_config(&state).await))
}

/// Set the program failover config.
#[utoipa::path(
    put,
    path = "/api/v1/failover",
    tag = "failover",
    request_body = muxshed_common::FailoverConfig,
    responses((status = 200, description = "Updated failover config", body = muxshed_common::FailoverConfig))
)]
pub async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(cfg): Json<FailoverConfig>,
) -> Result<Json<FailoverConfig>, ApiError> {
    if cfg.enabled {
        let fallback = cfg.fallback_source_id.ok_or_else(||
            MuxshedError::BadRequest("Select a standby source before enabling failover".into())
        )?;
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE id = ?")
            .bind(fallback.to_string())
            .fetch_one(&state.db)
            .await?;
        if exists == 0 {
            return Err(MuxshedError::BadRequest("Standby source no longer exists".into()).into());
        }
        if Some(fallback) == *state.program_intent.borrow() {
            return Err(MuxshedError::BadRequest("Standby cannot be the current live source".into()).into());
        }
    }
    let mut cfg = cfg;
    cfg.offline_grace_ms = cfg.offline_grace_ms.clamp(500, 60_000);
    cfg.recovery_grace_ms = cfg.recovery_grace_ms.clamp(500, 60_000);
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

/// All eligible fallback choices, including media-file sources hidden from
/// the normal live-ingress list. Never returns RTSP URLs or stream keys.
#[derive(Serialize)]
pub struct FallbackChoice {
    pub id: String,
    pub name: String,
}

pub async fn list_fallback_choices(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<FallbackChoice>>, ApiError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM sources ORDER BY name COLLATE NOCASE"
    ).fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(|(id, name)| FallbackChoice { id, name }).collect()))
}
