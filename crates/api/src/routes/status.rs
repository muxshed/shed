// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::PipelineState;

#[derive(Serialize)]
pub struct StatusResponse {
    pub pipeline: PipelineState,
}

pub async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, ApiError> {
    let pipeline = state.pipeline.state().await;
    Ok(Json(StatusResponse { pipeline }))
}
