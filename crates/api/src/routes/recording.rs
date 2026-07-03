// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{MuxshedError, PipelineState};

/// Start recording the program output.
#[utoipa::path(
    post,
    path = "/api/v1/record/start",
    tag = "recording",
    responses((status = 200, description = "Recording started"))
)]
pub async fn start(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    let current = state.pipeline.state().await;
    if !matches!(current, PipelineState::Live { .. }) {
        return Err(MuxshedError::BadRequest("pipeline must be live to record".to_string()).into());
    }

    let config = state.config.read().await;
    let path = config.data_dir.join("recordings");
    let filename = format!(
        "muxshed_{}.flv",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let full_path = path.join(filename);

    state.pipeline.start_recording(&full_path).await?;

    Ok(StatusCode::OK)
}

/// Stop the active recording.
#[utoipa::path(
    post,
    path = "/api/v1/record/stop",
    tag = "recording",
    responses((status = 200, description = "Recording stopped"))
)]
pub async fn stop(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    let rec = state.pipeline.recording_state().await;
    if !rec.recording {
        return Err(MuxshedError::BadRequest("not recording".to_string()).into());
    }

    state.pipeline.stop_recording().await?;

    Ok(StatusCode::OK)
}

#[derive(Serialize, ToSchema)]
pub struct RecordingResponse {
    pub recording: bool,
    pub path: Option<String>,
    pub started_at: Option<String>,
}

/// Current recording status.
#[utoipa::path(
    get,
    path = "/api/v1/record/status",
    tag = "recording",
    responses((status = 200, description = "Recording status", body = RecordingResponse))
)]
pub async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RecordingResponse>, ApiError> {
    let rec = state.pipeline.recording_state().await;
    Ok(Json(RecordingResponse {
        recording: rec.recording,
        path: rec.path.map(|p| p.display().to_string()),
        started_at: rec.started_at.map(|t| t.to_rfc3339()),
    }))
}
