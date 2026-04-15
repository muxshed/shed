// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
