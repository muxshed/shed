// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{PipelineState, SourceState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum WsEvent {
    PipelineState {
        #[serde(flatten)]
        state: PipelineState,
    },
    SourceState {
        id: Uuid,
        state: SourceState,
    },
    DestinationState {
        id: Uuid,
        state: String,
    },
    SceneChanged {
        scene_id: Uuid,
        method: String,
    },
    RecordingState {
        recording: bool,
        path: Option<String>,
    },
    TransitionStarted {
        stinger_id: Uuid,
        target_scene_id: Uuid,
    },
    TransitionComplete {
        scene_id: Uuid,
    },
    /// Program failover engaged or cleared (main source offline -> fallback).
    FailoverState {
        active: bool,
        fallback_source_id: Option<Uuid>,
        /// The operator-selected source the failover is standing in for.
        intent_source_id: Option<Uuid>,
    },
    ScheduleStarted { id: Uuid },
    ScheduleEnded { id: Uuid },
    ScheduleSkipped { id: Uuid, reason: String },
    Error {
        message: String,
        code: String,
    },
}
