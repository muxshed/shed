// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

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
    /// The source actually being broadcast (the fallback during a failover).
    pub program_source_id: Option<Uuid>,
    /// The operator-selected source (what they intend on program).
    pub program_intent_id: Option<Uuid>,
    pub preview_source_id: Option<Uuid>,
    /// True when the program is on the fallback because the intended source is offline.
    pub failover_active: bool,
}

pub async fn get_program(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProgramState>, ApiError> {
    let program = *state.program_source.borrow();
    let intent = *state.program_intent.borrow();
    let preview = *state.preview_source.read().await;
    Ok(Json(ProgramState {
        program_source_id: program,
        program_intent_id: intent,
        preview_source_id: preview,
        failover_active: *state.failover_active.borrow(),
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

    // Cutting to a single source replaces any active scene composite.
    crate::scene_compositor::stop_all_compositors(&state).await;
    let _ = state.program_intent.send(Some(id));

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

    let _ = state.program_intent.send(Some(preview_id));

    // Clear preview
    let mut p = state.preview_source.write().await;
    *p = None;

    let _ = state.ws_tx.send(WsEvent::SceneChanged {
        scene_id: preview_id,
        method: "cut".to_string(),
    });

    Ok(StatusCode::OK)
}
