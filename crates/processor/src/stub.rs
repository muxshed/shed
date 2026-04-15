// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use async_trait::async_trait;
use muxshed_common::{DelayConfig, Destination, MuxshedError, PipelineState, RecordingState, WsEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::controller::PipelineController;

pub struct StubPipelineController {
    state: Arc<Mutex<PipelineState>>,
    recording: Arc<Mutex<RecordingState>>,
    ws_tx: broadcast::Sender<WsEvent>,
}

impl StubPipelineController {
    pub fn new(ws_tx: broadcast::Sender<WsEvent>) -> Self {
        Self {
            state: Arc::new(Mutex::new(PipelineState::Idle)),
            recording: Arc::new(Mutex::new(RecordingState {
                recording: false,
                path: None,
                started_at: None,
            })),
            ws_tx,
        }
    }
}

#[async_trait]
impl PipelineController for StubPipelineController {
    async fn start(&self, _destinations: Vec<Destination>) -> Result<(), MuxshedError> {
        {
            let mut state = self.state.lock().await;
            *state = PipelineState::Starting;
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState {
            state: PipelineState::Starting,
        });

        let state = self.state.clone();
        let ws_tx = self.ws_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let live_state = PipelineState::Live {
                started_at: chrono::Utc::now(),
                active_scene: Uuid::nil(),
            };
            {
                let mut s = state.lock().await;
                *s = live_state.clone();
            }
            let _ = ws_tx.send(WsEvent::PipelineState { state: live_state });
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), MuxshedError> {
        {
            let mut state = self.state.lock().await;
            *state = PipelineState::Stopping;
        }
        let _ = self.ws_tx.send(WsEvent::PipelineState {
            state: PipelineState::Stopping,
        });

        let state = self.state.clone();
        let ws_tx = self.ws_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            {
                let mut s = state.lock().await;
                *s = PipelineState::Idle;
            }
            let _ = ws_tx.send(WsEvent::PipelineState {
                state: PipelineState::Idle,
            });
        });

        Ok(())
    }

    async fn state(&self) -> PipelineState {
        self.state.lock().await.clone()
    }

    async fn add_destination(&self, _dest: &Destination) -> Result<(), MuxshedError> {
        tracing::info!("stub: add_destination (no-op)");
        Ok(())
    }

    async fn remove_destination(&self, _id: &Uuid) -> Result<(), MuxshedError> {
        tracing::info!("stub: remove_destination (no-op)");
        Ok(())
    }

    async fn activate_scene(&self, scene_id: &Uuid) -> Result<(), MuxshedError> {
        tracing::info!("stub: activate_scene {}", scene_id);
        let mut state = self.state.lock().await;
        if let PipelineState::Live { started_at, .. } = state.clone() {
            *state = PipelineState::Live {
                started_at,
                active_scene: *scene_id,
            };
            let _ = self.ws_tx.send(WsEvent::PipelineState {
                state: state.clone(),
            });
        }
        Ok(())
    }

    async fn start_recording(&self, path: &PathBuf) -> Result<(), MuxshedError> {
        tracing::info!("stub: start_recording at {:?}", path);
        let mut rec = self.recording.lock().await;
        *rec = RecordingState {
            recording: true,
            path: Some(path.clone()),
            started_at: Some(chrono::Utc::now()),
        };
        let _ = self.ws_tx.send(WsEvent::RecordingState {
            recording: true,
            path: Some(path.display().to_string()),
        });
        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), MuxshedError> {
        tracing::info!("stub: stop_recording");
        let mut rec = self.recording.lock().await;
        *rec = RecordingState {
            recording: false,
            path: None,
            started_at: None,
        };
        let _ = self.ws_tx.send(WsEvent::RecordingState {
            recording: false,
            path: None,
        });
        Ok(())
    }

    async fn recording_state(&self) -> RecordingState {
        self.recording.lock().await.clone()
    }

    async fn set_delay(&self, config: &DelayConfig) -> Result<(), MuxshedError> {
        tracing::info!("stub: set_delay enabled={} duration={}ms", config.enabled, config.duration_ms);
        Ok(())
    }

    async fn trigger_bleep(&self) -> Result<(), MuxshedError> {
        tracing::info!("stub: trigger_bleep");
        let _ = self.ws_tx.send(WsEvent::BleepTriggered {
            at_ms: 0,
            source: "manual".to_string(),
        });
        Ok(())
    }

    async fn trigger_stinger_transition(
        &self,
        stinger_id: &Uuid,
        target_scene_id: &Uuid,
    ) -> Result<(), MuxshedError> {
        tracing::info!("stub: stinger transition stinger={} target={}", stinger_id, target_scene_id);
        let _ = self.ws_tx.send(WsEvent::TransitionStarted {
            stinger_id: *stinger_id,
            target_scene_id: *target_scene_id,
        });

        let ws_tx = self.ws_tx.clone();
        let state = self.state.clone();
        let scene_id = *target_scene_id;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            let mut s = state.lock().await;
            if let PipelineState::Live { started_at, .. } = s.clone() {
                *s = PipelineState::Live {
                    started_at,
                    active_scene: scene_id,
                };
                let _ = ws_tx.send(WsEvent::PipelineState { state: s.clone() });
            }
            let _ = ws_tx.send(WsEvent::TransitionComplete { scene_id });
        });

        Ok(())
    }
}
