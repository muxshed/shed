// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

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
    Browser { url: String },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub enabled: bool,
    pub token: String,
    pub title: String,
    pub logo_url: Option<String>,
    pub accent: Option<String>,
    pub password_protected: bool,
    pub watch_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub title: String,
    pub logo_url: Option<String>,
    pub accent: Option<String>,
    pub live: bool,
    pub requires_password: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};

    /// Serialize -> deserialize -> serialize and confirm the JSON is stable.
    fn roundtrips<T: Serialize + for<'de> Deserialize<'de>>(v: &T) {
        let s = serde_json::to_string(v).expect("serialize");
        let back: T = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(v).unwrap(),
            "roundtrip changed the value"
        );
    }

    #[test]
    fn source_kind_tags() {
        assert_eq!(to_value(SourceKind::Rtmp { stream_key: "k".into() }).unwrap()["type"], "rtmp");
        assert_eq!(to_value(SourceKind::Srt { port: 9000, passphrase: None }).unwrap()["type"], "srt");
        assert_eq!(to_value(SourceKind::WebRtc { token: "t".into() }).unwrap()["type"], "web_rtc");
        assert_eq!(to_value(SourceKind::TestPattern).unwrap()["type"], "test_pattern");
        assert_eq!(to_value(SourceKind::Browser { url: "u".into() }).unwrap()["type"], "browser");
        let mf = SourceKind::MediaFile {
            asset_id: Uuid::nil(),
            file_path: "/a.mp4".into(),
            loop_mode: "loop".into(),
        };
        assert_eq!(to_value(&mf).unwrap()["type"], "media_file");
    }

    #[test]
    fn source_kind_roundtrip_all_variants() {
        for k in [
            SourceKind::Rtmp { stream_key: "abc".into() },
            SourceKind::Srt { port: 9001, passphrase: Some("pass1234567".into()) },
            SourceKind::Srt { port: 9002, passphrase: None },
            SourceKind::WebRtc { token: "tok".into() },
            SourceKind::TestPattern,
            SourceKind::Browser { url: "https://example.com".into() },
            SourceKind::MediaFile {
                asset_id: Uuid::new_v4(),
                file_path: "/v.mp4".into(),
                loop_mode: "one_shot".into(),
            },
        ] {
            roundtrips(&k);
        }
    }

    #[test]
    fn source_state_serialization() {
        assert_eq!(to_value(SourceState::Live).unwrap(), json!("live"));
        assert_eq!(to_value(SourceState::Disconnected).unwrap(), json!("disconnected"));
        assert_eq!(to_value(SourceState::Connecting).unwrap(), json!("connecting"));
        let e = to_value(SourceState::Error("boom".into())).unwrap();
        assert_eq!(e["error"], "boom");
        let back: SourceState = serde_json::from_value(e).unwrap();
        assert_eq!(back, SourceState::Error("boom".into()));
    }

    #[test]
    fn source_roundtrip() {
        let s = Source {
            id: Uuid::new_v4(),
            name: "OBS".into(),
            kind: SourceKind::Browser { url: "https://x".into() },
            state: SourceState::Live,
        };
        let back: Source = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.name, "OBS");
        assert_eq!(back.state, SourceState::Live);
        assert!(matches!(back.kind, SourceKind::Browser { .. }));
    }

    #[test]
    fn scene_and_layers_roundtrip() {
        let scene = Scene {
            id: Uuid::new_v4(),
            name: "Main".into(),
            layers: vec![Layer {
                id: Uuid::new_v4(),
                source_id: Uuid::new_v4(),
                position: Position { x: 10, y: -5 },
                size: Size { width: 1920, height: 1080 },
                z_index: 2,
                opacity: 0.5,
            }],
        };
        let back: Scene = serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
        assert_eq!(back.layers.len(), 1);
        assert_eq!(back.layers[0].position.x, 10);
        assert_eq!(back.layers[0].position.y, -5);
        assert_eq!(back.layers[0].size.width, 1920);
        assert_eq!(back.layers[0].z_index, 2);
        assert!((back.layers[0].opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stinger_audio_tags() {
        assert_eq!(to_value(StingerAudio::Silent).unwrap(), json!("silent"));
        assert_eq!(to_value(StingerAudio::Overlay).unwrap(), json!("overlay"));
        assert_eq!(to_value(StingerAudio::Replace).unwrap(), json!("replace"));
        let duck = to_value(StingerAudio::Duck(0.3)).unwrap();
        assert!((duck["duck"].as_f64().unwrap() - 0.3).abs() < 1e-5);
    }

    #[test]
    fn destination_kind_tags() {
        assert_eq!(
            to_value(DestinationKind::Rtmp { url: "u".into(), stream_key: "k".into() }).unwrap()["type"],
            "rtmp"
        );
        assert_eq!(
            to_value(DestinationKind::Rtmps { url: "u".into(), stream_key: "k".into() }).unwrap()["type"],
            "rtmps"
        );
        assert_eq!(to_value(DestinationKind::Srt { url: "u".into() }).unwrap()["type"], "srt");
        assert_eq!(to_value(DestinationKind::Recording { path: "/r".into() }).unwrap()["type"], "recording");
    }

    #[test]
    fn pipeline_state_tags() {
        assert_eq!(to_value(PipelineState::Idle).unwrap()["state"], "idle");
        assert_eq!(to_value(PipelineState::Starting).unwrap()["state"], "starting");
        assert_eq!(to_value(PipelineState::Stopping).unwrap()["state"], "stopping");
        let live = to_value(PipelineState::Live {
            started_at: Utc::now(),
            active_scene: Uuid::new_v4(),
        })
        .unwrap();
        assert_eq!(live["state"], "live");
        assert!(live["started_at"].is_string());
        let err = to_value(PipelineState::Error { message: "x".into() }).unwrap();
        assert_eq!(err["state"], "error");
        assert_eq!(err["message"], "x");
    }

    #[test]
    fn api_scope_serialization() {
        assert_eq!(to_value(ApiScope::Read).unwrap(), json!("read"));
        assert_eq!(to_value(ApiScope::Control).unwrap(), json!("control"));
        assert_eq!(to_value(ApiScope::Admin).unwrap(), json!("admin"));
        let back: Vec<ApiScope> = serde_json::from_str(r#"["read","admin"]"#).unwrap();
        assert_eq!(back, vec![ApiScope::Read, ApiScope::Admin]);
    }

    #[test]
    fn recording_and_channel_roundtrip() {
        roundtrips(&RecordingState {
            recording: true,
            path: Some("/rec.mp4".into()),
            started_at: Some(Utc::now()),
        });
        roundtrips(&ChannelConfig {
            enabled: true,
            token: "tok".into(),
            title: "My Channel".into(),
            logo_url: None,
            accent: Some("#FFB000".into()),
            password_protected: false,
            watch_path: "/watch/tok".into(),
        });
        roundtrips(&ChannelInfo {
            title: "T".into(),
            logo_url: Some("/logo".into()),
            accent: None,
            live: true,
            requires_password: true,
        });
    }

    #[test]
    fn destination_and_stinger_roundtrip() {
        roundtrips(&Destination {
            id: Uuid::new_v4(),
            name: "YouTube".into(),
            kind: DestinationKind::Rtmps { url: "rtmps://a".into(), stream_key: "k".into() },
            enabled: true,
        });
        roundtrips(&StingerConfig {
            id: Uuid::new_v4(),
            name: "Swipe".into(),
            file_path: "/s.webm".into(),
            duration_ms: 1000,
            start_ms: 0,
            opaque_ms: 400,
            clear_ms: 600,
            end_ms: 1000,
            audio_behaviour: StingerAudio::Duck(0.2),
            thumbnail_path: "/s.png".into(),
        });
    }
}
