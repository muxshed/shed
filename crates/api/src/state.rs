// Licensed under the Business Source License 1.1 — see LICENSE.

use bytes::Bytes;
use muxshed_common::{MuxshedConfig, SourceState, WsEvent};
use muxshed_processor::PipelineController;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use uuid::Uuid;

use crate::egress::EgressManager;

pub struct AppState {
    pub pipeline: Arc<dyn PipelineController>,
    pub config: Arc<RwLock<MuxshedConfig>>,
    pub db: SqlitePool,
    pub ws_tx: broadcast::Sender<WsEvent>,
    pub source_states: RwLock<HashMap<Uuid, SourceState>>,
    pub media_relays: RwLock<HashMap<Uuid, broadcast::Sender<Bytes>>>,
    pub sequence_headers: RwLock<HashMap<Uuid, SequenceHeaders>>,
    pub source_media_info: RwLock<HashMap<Uuid, SourceMediaInfo>>,
    pub media_players: RwLock<HashMap<Uuid, tokio::process::Child>>,
    pub source_normalizers: RwLock<HashMap<Uuid, tokio::process::Child>>,
    pub srt_listeners: RwLock<HashMap<Uuid, tokio::process::Child>>,
    pub egress: EgressManager,
    /// Program output — the channel that egress and preview-program read from
    pub program_tx: broadcast::Sender<Bytes>,
    /// Which source provides video to program
    pub program_source: watch::Sender<Option<Uuid>>,
    /// Which source is in preview (next to go live)
    pub preview_source: RwLock<Option<Uuid>>,
    /// Audio routing: which sources are providing audio and at what level
    pub audio_routing: watch::Sender<AudioRouting>,
}

#[derive(Clone, Default)]
pub struct SequenceHeaders {
    pub video: Option<Bytes>,
    pub audio: Option<Bytes>,
    pub last_keyframe: Option<Bytes>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AudioChannelState {
    pub source_id: Uuid,
    pub muted: bool,
    pub volume: f32, // 0.0 to 1.0
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioRouting {
    /// Which source provides audio to program. None = follows video source.
    pub active_audio_source: Option<Uuid>,
    /// Per-source channel state
    pub channels: Vec<AudioChannelState>,
    /// If true, audio source changes when video source changes
    pub audio_follows_video: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SourceMediaInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    pub audio_sample_rate: Option<u32>,
    pub encoder: Option<String>,
}

impl AppState {
    pub async fn get_or_create_media_relay(
        &self,
        source_id: Uuid,
    ) -> broadcast::Sender<Bytes> {
        let mut relays = self.media_relays.write().await;
        relays
            .entry(source_id)
            .or_insert_with(|| broadcast::channel(4096).0)
            .clone()
    }

    pub async fn get_media_relay(
        &self,
        source_id: &Uuid,
    ) -> Option<broadcast::Sender<Bytes>> {
        let relays = self.media_relays.read().await;
        relays.get(source_id).cloned()
    }

    pub async fn remove_media_relay(&self, source_id: &Uuid) {
        let mut relays = self.media_relays.write().await;
        relays.remove(source_id);
    }
}
