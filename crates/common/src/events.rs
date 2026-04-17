// Licensed under the Business Source License 1.1 — see LICENSE.

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
    BleepTriggered {
        at_ms: u64,
        source: String,
    },
    DelayBuffer {
        live_ms: u64,
        output_ms: u64,
    },
    Error {
        message: String,
        code: String,
    },
}
