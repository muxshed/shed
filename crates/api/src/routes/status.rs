// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::PipelineState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct StatusResponse {
    pub pipeline: PipelineState,
}

/// Current pipeline status.
#[utoipa::path(
    get,
    path = "/api/v1/status",
    tag = "status",
    responses((status = 200, description = "Pipeline status", body = StatusResponse))
)]
pub async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, ApiError> {
    let pipeline = state.pipeline.state().await;
    Ok(Json(StatusResponse { pipeline }))
}
