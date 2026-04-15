// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BroadcastConfig {
    pub source_id: Option<Uuid>,
    pub scene_id: Option<Uuid>,
    pub start_stinger_id: Option<Uuid>,
    pub destination_ids: Vec<Uuid>,
    pub enable_delay: bool,
    pub delay_ms: u64,
    pub auto_record: bool,
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BroadcastConfig>, ApiError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings WHERE key = 'broadcast_config'")
            .fetch_optional(&state.db)
            .await?;

    let config = match row {
        Some((json,)) => serde_json::from_str(&json).unwrap_or_default(),
        None => BroadcastConfig::default(),
    };

    Ok(Json(config))
}

pub async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<BroadcastConfig>,
) -> Result<Json<BroadcastConfig>, ApiError> {
    let json = serde_json::to_string(&config)
        .map_err(|e| muxshed_common::MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('broadcast_config', ?)")
        .bind(&json)
        .execute(&state.db)
        .await?;

    Ok(Json(config))
}
