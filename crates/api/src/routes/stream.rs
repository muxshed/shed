// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::routes::broadcast::BroadcastConfig;
use crate::state::AppState;
use muxshed_common::{Destination, DestinationKind, MuxshedError, PipelineState};

pub async fn start(
    State(state): State<Arc<AppState>>,
    body: Option<Json<serde_json::Value>>,
) -> Result<StatusCode, ApiError> {
    let current = state.pipeline.state().await;
    if !matches!(current, PipelineState::Idle) {
        return Err(MuxshedError::BadRequest("pipeline is not idle".to_string()).into());
    }

    let saved_config: BroadcastConfig = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM settings WHERE key = 'broadcast_config'",
    )
    .fetch_optional(&state.db)
    .await?
    .and_then(|(json,)| serde_json::from_str(&json).ok())
    .unwrap_or_default();

    // Source: explicit param > current program source > broadcast config > first live source
    let current_program = *state.program_source.borrow();
    let source_id = body
        .as_ref()
        .and_then(|Json(v)| v.get("source_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok())
        .or(current_program)
        .or(saved_config.source_id);

    let source_id = match source_id {
        Some(id) => {
            let states = state.source_states.read().await;
            if states.get(&id) == Some(&muxshed_common::SourceState::Live) {
                id
            } else {
                return Err(MuxshedError::BadRequest(
                    format!("source {} is not live", id),
                ).into());
            }
        }
        None => {
            let states = state.source_states.read().await;
            states
                .iter()
                .find(|(_, s)| **s == muxshed_common::SourceState::Live)
                .map(|(id, _)| *id)
                .ok_or_else(|| {
                    MuxshedError::BadRequest(
                        "no live source available — connect OBS first".to_string(),
                    )
                })?
        }
    };

    // Destinations: broadcast config selection or all enabled
    let destinations = if !saved_config.destination_ids.is_empty() {
        let placeholders: Vec<String> = saved_config
            .destination_ids
            .iter()
            .map(|id| format!("'{}'", id))
            .collect();
        let query = format!(
            "SELECT id, name, kind, enabled FROM destinations WHERE id IN ({}) AND enabled = 1",
            placeholders.join(",")
        );
        let rows = sqlx::query_as::<_, DestRow>(&query)
            .fetch_all(&state.db)
            .await?;
        rows.into_iter().map(|r| r.into_destination()).collect::<Vec<_>>()
    } else {
        let rows = sqlx::query_as::<_, DestRow>(
            "SELECT id, name, kind, enabled FROM destinations WHERE enabled = 1",
        )
        .fetch_all(&state.db)
        .await?;
        rows.into_iter().map(|r| r.into_destination()).collect::<Vec<_>>()
    };

    if destinations.is_empty() {
        let total: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM destinations")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);
        let msg = if total == 0 {
            "no destinations configured — add one in Destinations page".to_string()
        } else if !saved_config.destination_ids.is_empty() {
            format!("none of the {} selected destinations are enabled", saved_config.destination_ids.len())
        } else {
            "no enabled destinations — enable at least one in Destinations page".to_string()
        };
        return Err(MuxshedError::BadRequest(msg).into());
    }

    tracing::info!("going live: source={}, destinations={}", source_id, destinations.len());

    if state.get_media_relay(&source_id).await.is_none() {
        return Err(MuxshedError::BadRequest("source is not streaming".to_string()).into());
    }

    if saved_config.enable_delay {
        let delay = muxshed_common::DelayConfig {
            enabled: true,
            duration_ms: saved_config.delay_ms,
            whisper_enabled: false,
        };
        let _ = state.pipeline.set_delay(&delay).await;
    }

    let output_config: Option<crate::routes::output::OutputConfig> =
        sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|(json,)| serde_json::from_str(&json).ok());

    let seq_headers = {
        let headers = state.sequence_headers.read().await;
        headers.get(&source_id).cloned()
    };

    // Start egress FIRST so it subscribes to program_tx before data flows
    if let Err(e) = state
        .egress
        .start(source_id, destinations.clone(), state.program_tx.clone(), output_config, seq_headers)
        .await
    {
        tracing::warn!("egress start error (continuing): {}", e);
    }

    // THEN set program source — program router wakes up and starts forwarding
    let _ = state.program_source.send(Some(source_id));
    tracing::info!("program source set to {}", source_id);

    if saved_config.auto_record {
        let config = state.config.read().await;
        let path = config.data_dir.join("recordings").join(format!(
            "muxshed_{}.flv",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        ));
        let _ = state.pipeline.start_recording(&path).await;
    }

    state.pipeline.start(destinations).await?;

    if let Some(scene_id) = saved_config.scene_id {
        let _ = state.pipeline.activate_scene(&scene_id).await;
    }

    Ok(StatusCode::OK)
}

pub async fn stop(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    let current = state.pipeline.state().await;
    if matches!(current, PipelineState::Idle) {
        return Err(MuxshedError::BadRequest("pipeline is already idle".to_string()).into());
    }

    state.egress.stop().await;
    let _ = state.program_source.send(None);
    let _ = state.pipeline.stop_recording().await;
    state.pipeline.stop().await?;

    Ok(StatusCode::OK)
}

#[derive(sqlx::FromRow)]
struct DestRow {
    id: String,
    name: String,
    kind: String,
    enabled: i32,
}

impl DestRow {
    fn into_destination(self) -> Destination {
        let kind = serde_json::from_str(&self.kind).unwrap_or(DestinationKind::Rtmp {
            url: String::new(),
            stream_key: String::new(),
        });
        Destination {
            id: self.id.parse().unwrap_or_default(),
            name: self.name,
            kind,
            enabled: self.enabled != 0,
        }
    }
}
