// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::{AppState, AudioRouting};
use muxshed_common::MuxshedError;

async fn persist_routing(state: &AppState, routing: &AudioRouting) {
    if let Ok(json) = serde_json::to_string(routing) {
        let _ = sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES ('audio_routing', ?)")
            .bind(&json)
            .execute(&state.db)
            .await;
    }
}

pub async fn get_routing(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AudioRouting>, ApiError> {
    let routing = state.audio_routing.borrow().clone();
    Ok(Json(routing))
}

pub async fn set_routing(
    State(state): State<Arc<AppState>>,
    Json(routing): Json<AudioRouting>,
) -> Result<Json<AudioRouting>, ApiError> {
    let _ = state.audio_routing.send(routing.clone());
    persist_routing(&state, &routing).await;
    Ok(Json(routing))
}

#[derive(Deserialize)]
pub struct SetAudioSourceRequest {
    pub source_id: Option<Uuid>,
}

pub async fn set_audio_source(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetAudioSourceRequest>,
) -> Result<StatusCode, ApiError> {
    if let Some(id) = body.source_id {
        let states = state.source_states.read().await;
        if states.get(&id) != Some(&muxshed_common::SourceState::Live) {
            return Err(MuxshedError::BadRequest("source is not live".to_string()).into());
        }
    }

    state.audio_routing.send_modify(|routing| {
        routing.active_audio_source = body.source_id;
        routing.audio_follows_video = body.source_id.is_none();
    });

    let routing = state.audio_routing.borrow().clone();
    persist_routing(&state, &routing).await;
    Ok(StatusCode::OK)
}

pub async fn toggle_follows_video(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    state.audio_routing.send_modify(|routing| {
        routing.audio_follows_video = !routing.audio_follows_video;
        if routing.audio_follows_video {
            routing.active_audio_source = None;
        }
    });

    let routing = state.audio_routing.borrow().clone();
    persist_routing(&state, &routing).await;
    Ok(StatusCode::OK)
}

pub async fn mute_source(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: Uuid = source_id.parse()
        .map_err(|_| MuxshedError::BadRequest("invalid uuid".to_string()))?;
    state.audio_routing.send_modify(|routing| {
        if let Some(ch) = routing.channels.iter_mut().find(|c| c.source_id == id) {
            ch.muted = true;
        }
    });

    let routing = state.audio_routing.borrow().clone();
    persist_routing(&state, &routing).await;
    Ok(StatusCode::OK)
}

pub async fn unmute_source(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: Uuid = source_id.parse()
        .map_err(|_| MuxshedError::BadRequest("invalid uuid".to_string()))?;
    state.audio_routing.send_modify(|routing| {
        if let Some(ch) = routing.channels.iter_mut().find(|c| c.source_id == id) {
            ch.muted = false;
        }
    });

    let routing = state.audio_routing.borrow().clone();
    persist_routing(&state, &routing).await;
    Ok(StatusCode::OK)
}
