// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{MuxshedError, WsEvent};

#[derive(Serialize)]
pub struct ProgramState {
    pub program_source_id: Option<Uuid>,
    pub preview_source_id: Option<Uuid>,
}

pub async fn get_program(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProgramState>, ApiError> {
    let program = *state.program_source.borrow();
    let preview = *state.preview_source.read().await;
    Ok(Json(ProgramState {
        program_source_id: program,
        preview_source_id: preview,
    }))
}

pub async fn set_preview(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| MuxshedError::BadRequest("invalid uuid".to_string()))?;

    let states = state.source_states.read().await;
    if states.get(&id) != Some(&muxshed_common::SourceState::Live) {
        return Err(MuxshedError::BadRequest("source is not live".to_string()).into());
    }
    drop(states);

    let mut preview = state.preview_source.write().await;
    *preview = Some(id);

    Ok(StatusCode::OK)
}

/// Hard cut: instantly switch program to the specified source.
/// Works in both studio mode (pre-live) and live mode.
pub async fn cut(
    State(state): State<Arc<AppState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| MuxshedError::BadRequest("invalid uuid".to_string()))?;

    let states = state.source_states.read().await;
    if states.get(&id) != Some(&muxshed_common::SourceState::Live) {
        return Err(MuxshedError::BadRequest("source is not live".to_string()).into());
    }
    drop(states);

    let _ = state.program_source.send(Some(id));

    let _ = state.ws_tx.send(WsEvent::SceneChanged {
        scene_id: id,
        method: "cut".to_string(),
    });

    Ok(StatusCode::OK)
}

/// Auto transition: cut preview to program. If no preview set, returns error.
pub async fn auto(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ApiError> {
    let preview = *state.preview_source.read().await;
    let preview_id = preview
        .ok_or_else(|| MuxshedError::BadRequest("no source in preview".to_string()))?;

    let _ = state.program_source.send(Some(preview_id));

    // Clear preview
    let mut p = state.preview_source.write().await;
    *p = None;

    let _ = state.ws_tx.send(WsEvent::SceneChanged {
        scene_id: preview_id,
        method: "cut".to_string(),
    });

    Ok(StatusCode::OK)
}
