// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MuxshedConfig {
    pub listen_addr: String,
    pub rtmp_port: u16,
    pub srt_port_range_start: u16,
    pub db_path: PathBuf,
    pub data_dir: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub log_level: String,
    /// Run without serving the web UI (API/CLI only). Set MUXSHED_HEADLESS=true.
    pub headless: bool,
    /// An admin API key preseeded from the environment (MUXSHED_API_KEY). In
    /// headless mode this is how the operator gets their first credential without
    /// the web setup wizard. Seeded (hashed) at startup, idempotently.
    pub bootstrap_api_key: Option<String>,
}

fn env_bool(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

impl MuxshedConfig {
    pub fn from_env() -> Self {
        Self {
            listen_addr: std::env::var("MUXSHED_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            rtmp_port: std::env::var("MUXSHED_RTMP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1935),
            srt_port_range_start: std::env::var("MUXSHED_SRT_PORT_START")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(9000),
            db_path: std::env::var("MUXSHED_DB_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("muxshed.db")),
            data_dir: std::env::var("MUXSHED_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data")),
            web_dir: std::env::var("MUXSHED_WEB_DIR")
                .ok()
                .map(PathBuf::from),
            log_level: std::env::var("MUXSHED_LOG_LEVEL")
                .unwrap_or_else(|_| "info".to_string()),
            headless: env_bool("MUXSHED_HEADLESS"),
            bootstrap_api_key: std::env::var("MUXSHED_API_KEY").ok().filter(|s| !s.is_empty()),
        }
    }
}
