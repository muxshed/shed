// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Server-side automatic commercial scheduler. Configuration survives restarts
//! through the existing SQLite settings table. Timers are monotonic and count
//! only while the broadcast is live. The existing ad player owns auto-return.

use std::sync::Arc;
use std::time::{Duration, Instant};
use axum::{extract::State, Json};
use muxshed_common::{MuxshedError, PipelineState};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

use crate::ads::PlayAdRequest;
use crate::error::ApiError;
use crate::state::AppState;

const SETTINGS_KEY: &str = "automatic_ad_schedule";
const DEFAULT_INTERVAL_MINUTES: u32 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomaticAdConfig {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub asset_id: Option<String>,
    #[serde(default)]
    pub playlist_ids: Vec<String>,
}

impl AutomaticAdConfig {
    fn effective_playlist(&self) -> Vec<String> {
        if !self.playlist_ids.is_empty() {
            return self.playlist_ids.clone();
        }
        self.asset_id.iter().cloned().collect()
    }
}

impl Default for AutomaticAdConfig {
    fn default() -> Self {
        Self { enabled: true, interval_minutes: DEFAULT_INTERVAL_MINUTES, asset_id: None, playlist_ids: Vec::new() }
    }
}

pub struct AutomaticAdRuntime {
    pub config: AutomaticAdConfig,
    pub next_due: Option<Instant>,
    pub skip_next: bool,
    pub playlist_cursor: usize,
}

impl AutomaticAdRuntime {
    pub fn new(config: AutomaticAdConfig) -> Self {
        Self { config, next_due: None, skip_next: false, playlist_cursor: 0 }
    }
}

#[derive(Serialize)]
pub struct AutomaticAdStatus {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub asset_id: Option<String>,
    pub broadcasting: bool,
    pub next_ad_in_seconds: Option<u64>,
    pub skip_next: bool,
    pub playlist_ids: Vec<String>,
    pub next_asset_id: Option<String>,
}

pub async fn load_config(db: &sqlx::SqlitePool) -> AutomaticAdConfig {
    let row = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(SETTINGS_KEY)
        .fetch_optional(db).await;
    match row {
        Ok(Some((json,))) => match serde_json::from_str::<AutomaticAdConfig>(&json) {
            Ok(value) if (1..=1440).contains(&value.interval_minutes) && value.playlist_ids.len() <= 20 => value,
            Ok(_) => {
                tracing::warn!("automatic ads: invalid stored interval; using safe defaults");
                AutomaticAdConfig::default()
            }
            Err(err) => {
                tracing::warn!("automatic ads: invalid stored settings: {}", err);
                AutomaticAdConfig::default()
            }
        },
        Ok(None) => AutomaticAdConfig::default(),
        Err(err) => {
            tracing::warn!("automatic ads: could not load settings: {}", err);
            AutomaticAdConfig::default()
        }
    }
}

async fn broadcast_is_live(state: &AppState) -> bool {
    matches!(state.pipeline.state().await, PipelineState::Live { .. })
}

async fn current_status(state: &AppState) -> AutomaticAdStatus {
    let broadcasting = broadcast_is_live(state).await;
    let guard = state.automatic_ads.lock().await;
    let config = &guard.config;
    let playlist = config.effective_playlist();
    let next_ad_in_seconds = if broadcasting && config.enabled && !playlist.is_empty() {
        guard.next_due.map(|due| due.saturating_duration_since(Instant::now()).as_secs())
    } else {
        None
    };
    AutomaticAdStatus {
        enabled: config.enabled,
        interval_minutes: config.interval_minutes,
        asset_id: config.asset_id.clone(),
        broadcasting,
        next_ad_in_seconds,
        skip_next: guard.skip_next,
        playlist_ids: playlist.clone(),
        next_asset_id: if playlist.is_empty() { None } else {
            Some(playlist[guard.playlist_cursor % playlist.len()].clone())
        },
    }
}

pub async fn get_schedule(State(state): State<Arc<AppState>>) -> Json<AutomaticAdStatus> {
    Json(current_status(&state).await)
}

pub async fn save_schedule(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AutomaticAdConfig>,
) -> Result<Json<AutomaticAdStatus>, ApiError> {
    if !(1..=1440).contains(&request.interval_minutes) {
        return Err(MuxshedError::BadRequest("automatic ad interval must be 1-1440 minutes".into()).into());
    }
    // Accept the original one-ad configuration without losing existing schedules.
    let mut request = request;
    if request.playlist_ids.is_empty() {
        if let Some(id) = request.asset_id.clone() {
            request.playlist_ids.push(id);
        }
    }
    if request.playlist_ids.len() > 20 {
        return Err(MuxshedError::BadRequest("commercial playlist is limited to 20 videos".into()).into());
    }
    let mut seen = std::collections::HashSet::new();
    for asset_id in &request.playlist_ids {
        let parsed = Uuid::parse_str(asset_id)
            .map_err(|_| MuxshedError::BadRequest("invalid commercial asset id".into()))?;
        if !seen.insert(parsed) {
            return Err(MuxshedError::BadRequest("duplicate commercial in playlist".into()).into());
        }
        let found = sqlx::query_as::<_, (String,)>(
            "SELECT id FROM assets WHERE id = ? AND asset_type = 'video'",
        )
        .bind(parsed.to_string())
        .fetch_optional(&state.db).await?;
        if found.is_none() {
            return Err(MuxshedError::BadRequest("select an existing Library video".into()).into());
        }
    }
    if request.enabled && request.playlist_ids.is_empty() {
        return Err(MuxshedError::BadRequest("choose at least one commercial before enabling automatic ads".into()).into());
    }
    // Keep asset_id populated so older read-only clients continue to function.
    request.asset_id = request.playlist_ids.first().cloned();

    let json = serde_json::to_string(&request)
        .map_err(|e| MuxshedError::Internal(format!("automatic ad settings: {e}")))?;
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(SETTINGS_KEY)
        .bind(json)
        .execute(&state.db).await?;

    let mut guard = state.automatic_ads.lock().await;
    guard.config = request;
    guard.next_due = None; // new 30-minute countdown begins while on air
    guard.skip_next = false;
    guard.playlist_cursor = 0;
    drop(guard);
    Ok(Json(current_status(&state).await))
}

pub async fn skip_next(State(state): State<Arc<AppState>>) -> Json<AutomaticAdStatus> {
    let mut guard = state.automatic_ads.lock().await;
    if guard.config.enabled && guard.config.asset_id.is_some() {
        guard.skip_next = true;
    }
    drop(guard);
    Json(current_status(&state).await)
}

/// Wait 30 minutes of an active broadcast before the first automatic ad.
/// Pause/reset if broadcast stops. Do not double-play during an active ad.
/// The current camera remains connected; ads::play handles one-shot playback
/// and restores the exact previous program source and audio on EOF.
pub async fn run_automatic_ads(state: Arc<AppState>) {
    loop {
        sleep(Duration::from_secs(1)).await;
        let broadcasting = broadcast_is_live(&state).await;
        let has_program = state.program_intent.borrow().is_some();
        let now = Instant::now();
        let mut play_asset: Option<String> = None;
        {
            let mut guard = state.automatic_ads.lock().await;
            if !broadcasting || !has_program || !guard.config.enabled || guard.config.effective_playlist().is_empty() {
                guard.next_due = None;
                if !broadcasting { guard.skip_next = false; }
                continue;
            }
            let interval = Duration::from_secs(guard.config.interval_minutes as u64 * 60);
            match guard.next_due {
                None => guard.next_due = Some(now + interval),
                Some(due) if now >= due => {
                    guard.next_due = Some(now + interval);
                    if guard.skip_next {
                        guard.skip_next = false;
                        tracing::info!("automatic ads: scheduled break skipped");
                    } else {
                        let playlist = guard.config.effective_playlist();
                        play_asset = Some(playlist[guard.playlist_cursor % playlist.len()].clone());
                    }
                }
                _ => {}
            }
        }
        if let Some(asset_id) = play_asset {
            if state.ad_session.lock().await.is_some() {
                tracing::info!("automatic ads: commercial already in progress; skipping this interval");
                continue;
            }
            let request = PlayAdRequest { asset_id };
            match crate::ads::play(State(state.clone()), Json(request)).await {
                Ok(_) => {
                    // Only advance the playlist after a successful start.
                    let mut guard = state.automatic_ads.lock().await;
                    let playlist = guard.config.effective_playlist();
                    if !playlist.is_empty() {
                        guard.playlist_cursor = (guard.playlist_cursor + 1) % playlist.len();
                    }
                    tracing::info!("automatic ads: commercial break started");
                },
                Err(err) => {
                    tracing::warn!("automatic ads: commercial could not start: {}", err.0);
                    // A deleted or unreadable video should not block all later sponsors.
                    let mut guard = state.automatic_ads.lock().await;
                    let playlist = guard.config.effective_playlist();
                    if playlist.len() > 1 {
                        guard.playlist_cursor = (guard.playlist_cursor + 1) % playlist.len();
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_single_ad_remains_valid() {
        let old = r#"{"enabled":true,"interval_minutes":30,"asset_id":"old-id"}"#;
        let config: AutomaticAdConfig = serde_json::from_str(old).unwrap();
        assert_eq!(config.effective_playlist(), vec!["old-id"]);
    }

    #[test]
    fn playlist_overrides_legacy_single_asset() {
        let config = AutomaticAdConfig {
            enabled: true,
            interval_minutes: 30,
            asset_id: Some("old".into()),
            playlist_ids: vec!["sponsor-a".into(), "sponsor-b".into()],
        };
        assert_eq!(config.effective_playlist(), vec!["sponsor-a", "sponsor-b"]);
    }
}
