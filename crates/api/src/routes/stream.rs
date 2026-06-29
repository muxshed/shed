// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

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

    // No external (RTMP/SRT) destinations are required to go live: the public Channel
    // watch page is always an available output, so the program can always be broadcast
    // somewhere. An empty destination list is valid — the pipeline still produces the
    // program output (and the channel HLS, when enabled).
    if destinations.is_empty() {
        tracing::info!("going live with no external destinations — channel/watch page output only");
    }

    tracing::info!("going live: source={}, destinations={}", source_id, destinations.len());

    if state.get_media_relay(&source_id).await.is_none() {
        return Err(MuxshedError::BadRequest("source is not streaming".to_string()).into());
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
    let _ = state.program_intent.send(Some(source_id));
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

    // Start the public Channel HLS output (ffmpeg) if the channel is enabled, so the
    // watch page goes live as soon as the studio goes on air.
    ensure_channel_hls(&state).await;

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
    state.channel_hls.stop().await;
    let _ = state.program_intent.send(None);
    let _ = state.pipeline.stop_recording().await;
    state.pipeline.stop().await?;

    Ok(StatusCode::OK)
}

/// Start or stop the public Channel HLS output to match the channel's `enabled` flag
/// and whether the studio is currently broadcasting. Safe to call on go-live and
/// whenever the channel config (enabled/token) changes.
pub async fn ensure_channel_hls(state: &Arc<AppState>) {
    let row = sqlx::query_as::<_, (i64, String)>("SELECT enabled, token FROM channel WHERE id = 1")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    let broadcasting = !matches!(
        state.pipeline.state().await,
        PipelineState::Idle | PipelineState::Stopping
    );

    match row {
        Some((enabled, token)) if enabled != 0 && broadcasting => {
            let data_dir = state.config.read().await.data_dir.clone();
            let output_config = load_output_config(state).await;
            let program_source = *state.program_source.borrow();
            let seq_headers = match program_source {
                Some(id) => state.sequence_headers.read().await.get(&id).cloned(),
                None => None,
            };
            if let Err(e) = state
                .channel_hls
                .start(
                    &token,
                    &data_dir,
                    state.program_tx.clone(),
                    output_config,
                    seq_headers,
                )
                .await
            {
                tracing::warn!("channel HLS start failed: {}", e);
            }
        }
        _ => state.channel_hls.stop().await,
    }
}

async fn load_output_config(state: &Arc<AppState>) -> Option<crate::routes::output::OutputConfig> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(json,)| serde_json::from_str(&json).ok())
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
