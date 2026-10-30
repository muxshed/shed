// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.
//! Opt-in broadcast automation. The safe default is monitor-only.
//! External alerts and authenticated YouTube health are NOT claimed here.

use axum::{extract::State, Json};
use muxshed_common::{MuxshedError, PipelineState, SourceState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const CONFIG_KEY: &str = "broadcast_automation_v1";
const SPONSOR_TEXT_PATH: &str = "/tmp/muxshed-sponsor-overlay.txt";
const WEATHER_TEXT_PATH: &str = "/tmp/muxshed-weather-popup.txt";
const FLAG_PATH: &str = "/tmp/muxshed-broadcast-overlay.enabled";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    pub weather_enabled: bool,
    pub weather_city: String,
    pub weather_latitude: f64,
    pub weather_longitude: f64,
    pub sponsor_enabled: bool,
    pub sponsor_text: String,
    pub broadcast_overlay_enabled: bool,
    // When enabled, sponsor remains on screen. Weather shows on an interval.
    pub weather_popup_interval_minutes: u32,
    pub weather_popup_duration_seconds: u32,
    pub auto_switch_enabled: bool,
    pub switch_interval_minutes: u32,
    pub switch_source_ids: Vec<Uuid>,
    pub alerts_enabled: bool,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            weather_enabled: false, weather_city: "Kingston".into(),
            weather_latitude: 17.997, weather_longitude: -76.7936,
            sponsor_enabled: false, sponsor_text: String::new(),
            broadcast_overlay_enabled: false,
            weather_popup_interval_minutes: 10, weather_popup_duration_seconds: 15,
            auto_switch_enabled: false,
            switch_interval_minutes: 30, switch_source_ids: Vec::new(),
            alerts_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WeatherReading {
    pub temperature_c: f64,
    pub humidity_percent: f64,
    pub wind_kmh: f64,
    pub rainfall_mm: f64,
    pub updated_at_utc: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutomationEvent {
    pub at_utc: String,
    pub kind: String,
    pub message: String,
}

pub struct AutomationRuntime {
    pub config: AutomationConfig,
    pub weather: Option<WeatherReading>,
    pub events: Vec<AutomationEvent>,
    pub config_version: u64,
    pub weather_popup_active: bool,
    pub weather_popup_next_seconds: Option<u64>,
}
impl AutomationRuntime {
    pub fn new(config: AutomationConfig) -> Self {
        Self {
            config, weather: None, events: Vec::new(), config_version: 0,
            weather_popup_active: false, weather_popup_next_seconds: None,
        }
    }
}

#[derive(Serialize)]
pub struct AutomationStatus {
    pub config: AutomationConfig,
    pub weather: Option<WeatherReading>,
    pub events: Vec<AutomationEvent>,
    pub overlay_note: String,
    pub weather_popup_active: bool,
    pub weather_popup_next_seconds: Option<u64>,
}

pub async fn load_config(db: &sqlx::SqlitePool) -> AutomationConfig {
    let row = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(CONFIG_KEY).fetch_optional(db).await;
    match row {
        Ok(Some((json,))) => serde_json::from_str(&json).unwrap_or_default(),
        _ => AutomationConfig::default(),
    }
}

fn checked(config: &AutomationConfig) -> Result<(), MuxshedError> {
    if !config.weather_latitude.is_finite() || !(-90.0..=90.0).contains(&config.weather_latitude)
        || !config.weather_longitude.is_finite() || !(-180.0..=180.0).contains(&config.weather_longitude) {
        return Err(MuxshedError::BadRequest("invalid weather coordinates".into()));
    }
    if config.weather_city.trim().is_empty() || config.weather_city.chars().count() > 48
        || config.sponsor_text.chars().count() > 100 || (config.sponsor_text.contains('\n') || config.sponsor_text.contains('\r')) {
        return Err(MuxshedError::BadRequest("invalid city or sponsor text".into()));
    }
    if !(1..=120).contains(&config.weather_popup_interval_minutes)
        || !(5..=60).contains(&config.weather_popup_duration_seconds)
        || config.weather_popup_duration_seconds >= config.weather_popup_interval_minutes * 60 {
        return Err(MuxshedError::BadRequest("invalid weather popup timing".into()));
    }
    if !(1..=1440).contains(&config.switch_interval_minutes) || config.switch_source_ids.len() > 20 {
        return Err(MuxshedError::BadRequest("invalid camera rotation interval or source list".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for source in &config.switch_source_ids {
        if !seen.insert(source) {
            return Err(MuxshedError::BadRequest("duplicate camera source".into()));
        }
    }
    if config.auto_switch_enabled && config.switch_source_ids.len() < 2 {
        return Err(MuxshedError::BadRequest("select at least two cameras for rotation".into()));
    }
    Ok(())
}

pub fn prepare_overlay(config: &AutomationConfig) {
    let _ = std::fs::write(SPONSOR_TEXT_PATH, "");
    let _ = std::fs::write(WEATHER_TEXT_PATH, "");
    if config.broadcast_overlay_enabled {
        let _ = std::fs::write(FLAG_PATH, "1");
    } else {
        let _ = std::fs::remove_file(FLAG_PATH);
    }
}

async fn status_for(state: &AppState) -> AutomationStatus {
    let runtime = state.automation.lock().await;
    AutomationStatus {
        config: runtime.config.clone(), weather: runtime.weather.clone(),
        events: runtime.events.iter().rev().take(30).cloned().collect(),
        overlay_note: "Sponsor stays on; weather pops up on schedule and hides during ads. The updated graphics layout needs the next outbound encoder start; text and popup timing update without restarting. YouTube-side health is a separate integration.".into(),
        weather_popup_active: runtime.weather_popup_active,
        weather_popup_next_seconds: runtime.weather_popup_next_seconds,
    }
}

pub async fn get_status(State(state): State<Arc<AppState>>) -> Json<AutomationStatus> {
    Json(status_for(&state).await)
}

pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<AutomationConfig>,
) -> Result<Json<AutomationStatus>, ApiError> {
    checked(&config)?;
    for id in &config.switch_source_ids {
        let found = sqlx::query_as::<_, (String,)>("SELECT id FROM sources WHERE id = ?")
            .bind(id.to_string()).fetch_optional(&state.db).await?;
        if found.is_none() {
            return Err(MuxshedError::BadRequest("camera source no longer exists".into()).into());
        }
    }
    let payload = serde_json::to_string(&config)
        .map_err(|err| MuxshedError::Internal(err.to_string()))?;
    sqlx::query("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
        .bind(CONFIG_KEY).bind(payload).execute(&state.db).await?;
    {
        let mut rt = state.automation.lock().await;
        rt.config = config.clone();
        rt.config_version += 1;
        rt.weather = None; // don't display old weather for another location
        rt.weather_popup_active = false;
        rt.weather_popup_next_seconds = None;
    }
    // Never restart an active outbound encoder from a config save.
    // Clearing the text immediately hides overlays already running.
    prepare_overlay(&config);
    Ok(Json(status_for(&state).await))
}

fn is_live(state: &AppState) -> impl std::future::Future<Output = bool> + '_ {
    async move { matches!(state.pipeline.state().await, PipelineState::Live { .. }) }
}

async fn fetch_weather(config: &AutomationConfig) -> Result<WeatherReading, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,relative_humidity_2m,precipitation,wind_speed_10m&timezone=auto",
        config.weather_latitude, config.weather_longitude
    );
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--connect-timeout", "4", "--max-time", "8", &url])
        .output().await.map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!("weather service HTTP request failed: {}", output.status));
    }
    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| err.to_string())?;
    let current = response.get("current").ok_or("missing weather conditions")?;
    let number = |name: &str| -> Result<f64, String> {
        current.get(name).and_then(|v| v.as_f64())
            .filter(|v| v.is_finite()).ok_or(format!("missing {}", name))
    };
    Ok(WeatherReading {
        temperature_c: number("temperature_2m")?,
        humidity_percent: number("relative_humidity_2m")?,
        wind_kmh: number("wind_speed_10m")?,
        rainfall_mm: number("precipitation")?,
        updated_at_utc: chrono::Utc::now().to_rfc3339(),
    })
}

fn sanitize_overlay(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control() && *ch != '%' && *ch != '\\')
        .take(100).collect()
}

// Atomically replace each textfile. FFmpeg's drawtext reload=1 reads the
// latest complete file without restarting the outbound RTMP connection.
fn atomic_text(path: &str, text: &str) {
    let tmp = format!("{}.new", path);
    if std::fs::write(&tmp, text).is_ok() {
        if let Err(err) = std::fs::rename(&tmp, path) {
            tracing::warn!("broadcast overlay file update failed: {}", err);
        }
    }
}

fn write_overlay(config: &AutomationConfig, popup_weather: Option<&WeatherReading>) {
    let sponsor = if config.broadcast_overlay_enabled && config.sponsor_enabled
        && !config.sponsor_text.trim().is_empty() {
        // Prevent excessively long text from overflowing the picture.
        format!("SPONSORED BY  {}", sanitize_overlay(&config.sponsor_text).chars().take(45).collect::<String>())
    } else { String::new() };
    let weather = if config.broadcast_overlay_enabled && config.weather_enabled {
        if let Some(w) = popup_weather {
            format!("{}  |  {:.0} C\nHumidity {:.0}%  Wind {:.0} km/h\nRain {:.1} mm",
                sanitize_overlay(&config.weather_city).chars().take(30).collect::<String>(),
                w.temperature_c, w.humidity_percent, w.wind_kmh, w.rainfall_mm)
        } else { String::new() }
    } else { String::new() };
    atomic_text(SPONSOR_TEXT_PATH, &sponsor);
    atomic_text(WEATHER_TEXT_PATH, &weather);
}

async fn log_event(state: &AppState, kind: &str, message: &str) {
    let mut runtime = state.automation.lock().await;
    runtime.events.push(AutomationEvent {
        at_utc: chrono::Utc::now().to_rfc3339(),
        kind: kind.into(), message: message.into(),
    });
    if runtime.events.len() > 100 {
        let extra = runtime.events.len() - 100;
        runtime.events.drain(0..extra);
    }
}

/// Conservative server loop. No feature starts automatically until configured.
pub async fn run(state: Arc<AppState>) {
    let mut last_weather_check = Instant::now() - Duration::from_secs(601);
    let mut weather_last_ok: Option<Instant> = None;
    let mut last_config_version = u64::MAX;
    let mut switch_due: Option<Instant> = None;
    let mut source_was_offline: Option<bool> = None;
    let mut last_overlay_text = String::new();
    let mut weather_popup_due: Option<Instant> = None;
    let mut weather_popup_ends: Option<Instant> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let (cfg, version) = {
            let rt = state.automation.lock().await;
            (rt.config.clone(), rt.config_version)
        };
        if version != last_config_version {
            last_config_version = version;
            // Save clears the overlay files: rewrite both even if text is identical.
            last_overlay_text.clear();
            switch_due = None;
            source_was_offline = None;
            last_weather_check = Instant::now() - Duration::from_secs(601);
            weather_last_ok = None;
            weather_popup_due = None;
            weather_popup_ends = None;
        }
        if cfg.weather_enabled && last_weather_check.elapsed() >= Duration::from_secs(600) {
            last_weather_check = Instant::now();
            match fetch_weather(&cfg).await {
                Ok(reading) => {
                    state.automation.lock().await.weather = Some(reading);
                    weather_last_ok = Some(Instant::now());
                },
                Err(error) => tracing::warn!("weather: conditions unavailable: {}", error),
            }
        }
        let weather = if weather_last_ok.map(|t| t.elapsed() < Duration::from_secs(1800)).unwrap_or(false) {
            state.automation.lock().await.weather.clone()
        } else { None };
        // Weather pop-ups run on a monotonic timer while broadcasting. The
        // sponsor remains independent and visible throughout the interval.
        // Never show weather over an ad or active failover.
        let broadcast_live = is_live(&state).await;
        let ad_active = state.ad_session.lock().await.is_some();
        let failover_active = *state.failover_active.borrow();
        let weather = if weather_last_ok.map(|t| t.elapsed() < Duration::from_secs(1800)).unwrap_or(false) {
            state.automation.lock().await.weather.clone()
        } else { None };
        let now = Instant::now();
        let eligible = broadcast_live && cfg.broadcast_overlay_enabled && cfg.weather_enabled;
        if !eligible {
            weather_popup_due = None;
            weather_popup_ends = None;
        } else {
            let interval = Duration::from_secs(cfg.weather_popup_interval_minutes as u64 * 60);
            if weather_popup_due.is_none() { weather_popup_due = Some(now + interval); }
            if weather_popup_ends.is_some_and(|end| now >= end) {
                weather_popup_ends = None;
            }
            if weather_popup_ends.is_none() && weather_popup_due.is_some_and(|due| now >= due) {
                if weather.is_some() && !ad_active && !failover_active {
                    weather_popup_ends = Some(now + Duration::from_secs(cfg.weather_popup_duration_seconds as u64));
                    weather_popup_due = Some(now + interval);
                } else {
                    // Retry later if weather is stale or another broadcast
                    // action owns the screen. Do not overlap a commercial.
                    weather_popup_due = Some(now + Duration::from_secs(30));
                }
            }
        }
        // Cancel the current weather display when a commercial or failover
        // takes the program. The countdown continues for the next cycle.
        if ad_active || failover_active { weather_popup_ends = None; }
        let popup_active = weather_popup_ends.is_some_and(|end| now < end)
            && !ad_active && !failover_active && weather.is_some();
        {
            let mut rt = state.automation.lock().await;
            rt.weather_popup_active = popup_active;
            rt.weather_popup_next_seconds = if eligible && !popup_active {
                weather_popup_due.map(|due| due.saturating_duration_since(now).as_secs())
            } else { None };
        }
        let effective_text = format!("{}|{}|{}|{}|{}|{}|{}|{:?}",
            cfg.broadcast_overlay_enabled, cfg.sponsor_enabled, cfg.sponsor_text,
            cfg.weather_enabled, cfg.weather_city, popup_active,
            cfg.weather_popup_interval_minutes,
            if popup_active { weather.as_ref().map(|w| &w.updated_at_utc) } else { None });
        if effective_text != last_overlay_text {
            write_overlay(&cfg, if popup_active { weather.as_ref() } else { None });
            last_overlay_text = effective_text;
        }
        if !broadcast_live {
            switch_due = None;
            source_was_offline = None;
            continue;
        }
        if cfg.alerts_enabled {
            let intended = *state.program_intent.borrow();
            if let Some(id) = intended {
                let offline = state.source_states.read().await.get(&id) != Some(&SourceState::Live);
                if source_was_offline != Some(offline) {
                    if source_was_offline.is_some() {
                        log_event(&state, if offline { "source_offline" } else { "source_recovered" },
                            if offline { "Program camera went offline" } else { "Program camera recovered" }).await;
                    }
                    source_was_offline = Some(offline);
                }
            }
        }
        if !cfg.auto_switch_enabled || cfg.switch_source_ids.len() < 2 || *state.failover_active.borrow()
            || state.ad_session.lock().await.is_some() {
            switch_due = None;
            continue;
        }
        let now = Instant::now();
        let interval = Duration::from_secs(cfg.switch_interval_minutes as u64 * 60);
        if switch_due.is_none() { switch_due = Some(now + interval); }
        if !switch_due.map(|t| now >= t).unwrap_or(false) { continue; }
        switch_due = Some(now + interval);
        let states: HashMap<Uuid, SourceState> = state.source_states.read().await.clone();
        let active = *state.program_intent.borrow();
        let index = active.and_then(|id| cfg.switch_source_ids.iter().position(|candidate| *candidate == id));
        let start = index.map(|i| i + 1).unwrap_or(0);
        for offset in 0..cfg.switch_source_ids.len() {
            let id = cfg.switch_source_ids[(start + offset) % cfg.switch_source_ids.len()];
            if Some(id) == active || states.get(&id) != Some(&SourceState::Live) { continue; }
            // Same safe path as a manual camera cut. This implementation may
            // reconnect YouTube on a source splice, so test off-air first.
            let _ = state.program_intent.send(Some(id));
            tracing::info!("automation: scheduled camera switched to {}", id);
            log_event(&state, "camera_switch", "Scheduled camera transition completed").await;
            break;
        }
    }
}
