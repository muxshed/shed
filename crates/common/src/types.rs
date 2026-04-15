// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub name: String,
    pub kind: SourceKind,
    pub state: SourceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    Rtmp { stream_key: String },
    Srt { port: u16, passphrase: Option<String> },
    WebRtc { token: String },
    TestPattern,
    MediaFile {
        asset_id: Uuid,
        file_path: PathBuf,
        loop_mode: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Disconnected,
    Connecting,
    Live,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: Uuid,
    pub name: String,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    pub source_id: Uuid,
    pub position: Position,
    pub size: Size,
    pub z_index: u32,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StingerConfig {
    pub id: Uuid,
    pub name: String,
    pub file_path: PathBuf,
    pub duration_ms: u64,
    pub start_ms: u64,
    pub opaque_ms: u64,
    pub clear_ms: u64,
    pub end_ms: u64,
    pub audio_behaviour: StingerAudio,
    pub thumbnail_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StingerAudio {
    Silent,
    Duck(f32),
    Overlay,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub id: Uuid,
    pub name: String,
    pub kind: DestinationKind,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DestinationKind {
    Rtmp { url: String, stream_key: String },
    Rtmps { url: String, stream_key: String },
    Srt { url: String },
    Recording { path: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PipelineState {
    Idle,
    Starting,
    Live {
        started_at: DateTime<Utc>,
        active_scene: Uuid,
    },
    Transitioning {
        stinger_id: Uuid,
        target_scene: Uuid,
    },
    Stopping,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayConfig {
    pub enabled: bool,
    pub duration_ms: u64,
    pub whisper_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub scopes: Vec<ApiScope>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiScope {
    Read,
    Control,
    Admin,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingState {
    pub recording: bool,
    pub path: Option<PathBuf>,
    pub started_at: Option<DateTime<Utc>>,
}
