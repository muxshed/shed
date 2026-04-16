// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use async_trait::async_trait;
use muxshed_common::{DelayConfig, Destination, MuxshedError, PipelineState, RecordingState};
use std::path::Path;
use uuid::Uuid;

#[async_trait]
pub trait PipelineController: Send + Sync {
    async fn start(&self, destinations: Vec<Destination>) -> Result<(), MuxshedError>;
    async fn stop(&self) -> Result<(), MuxshedError>;
    async fn state(&self) -> PipelineState;
    async fn add_destination(&self, dest: &Destination) -> Result<(), MuxshedError>;
    async fn remove_destination(&self, id: &Uuid) -> Result<(), MuxshedError>;
    async fn activate_scene(&self, scene_id: &Uuid) -> Result<(), MuxshedError>;
    async fn start_recording(&self, path: &Path) -> Result<(), MuxshedError>;
    async fn stop_recording(&self) -> Result<(), MuxshedError>;
    async fn recording_state(&self) -> RecordingState;
    async fn set_delay(&self, config: &DelayConfig) -> Result<(), MuxshedError>;
    async fn trigger_bleep(&self) -> Result<(), MuxshedError>;
    async fn trigger_stinger_transition(
        &self,
        stinger_id: &Uuid,
        target_scene_id: &Uuid,
    ) -> Result<(), MuxshedError>;
}
