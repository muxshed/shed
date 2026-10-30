// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Conservative local encoder watchdog. It inspects the FFmpeg process and
//! stdin progress, not YouTube's remote ingest health. Automatic recovery is
//! disabled by default to avoid surprise disruption to an existing broadcast.

use std::sync::Arc;
use std::time::{Duration, Instant};
use axum::{extract::State, Json};
use muxshed_common::PipelineState;
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;
use crate::error::ApiError;
use crate::state::AppState;

const CONFIG_KEY: &str = "reliability_watchdog_config";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    pub automatic_restart: bool,
    pub stale_after_seconds: u64,
    pub cooldown_seconds: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self { automatic_restart: false, stale_after_seconds: 30, cooldown_seconds: 120 }
    }
}

pub async fn load_config(db: &sqlx::SqlitePool) -> WatchdogConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(CONFIG_KEY)
        .fetch_optional(db).await.ok().flatten()
        .and_then(|(json,)| serde_json::from_str::<WatchdogConfig>(&json).ok())
        .map(normalize)
        .unwrap_or_default()
}

fn normalize(mut cfg: WatchdogConfig) -> WatchdogConfig {
    cfg.stale_after_seconds = cfg.stale_after_seconds.clamp(15, 300);
    cfg.cooldown_seconds = cfg.cooldown_seconds.clamp(90, 3600);
    cfg
}

#[derive(Serialize)]
pub struct ReliabilityStatus {
    pub broadcasting: bool,
    pub program_source_id: Option<Uuid>,
    pub intended_source_id: Option<Uuid>,
    pub failover_active: bool,
    pub program_relay_present: bool,
    pub failover_configured: bool,
    pub watchdog: WatchdogConfig,
    pub egress: Vec<crate::egress::EgressHealth>,
    pub note: &'static str,
}

pub async fn snapshot(state: &AppState) -> ReliabilityStatus {
    let broadcasting = matches!(state.pipeline.state().await, PipelineState::Live { .. });
    let program = *state.program_source.borrow();
    let intended = *state.program_intent.borrow();
    let relay_present = match program {
        Some(id) => state.get_media_relay(&id).await.is_some(),
        None => false,
    };
    let failover_config = crate::failover::load_config(state).await;
    let watchdog = load_config(&state.db).await;
    let egress = state.egress.health().await;
    ReliabilityStatus {
        broadcasting,
        program_source_id: program,
        intended_source_id: intended,
        failover_active: *state.failover_active.borrow(),
        program_relay_present: relay_present,
        failover_configured: failover_config.enabled && failover_config.fallback_source_id.is_some(),
        watchdog,
        egress,
        note: "FFmpeg/process telemetry only; remote YouTube ingest health is not independently verified",
    }
}

pub async fn run_watchdog(state: Arc<AppState>) {
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_restart: Option<Instant> = None;
    let mut restart_history: Vec<Instant> = Vec::new();
    let mut problem_reported = false;
    loop {
        tick.tick().await;
        let is_live = matches!(state.pipeline.state().await, PipelineState::Live { .. });
        if !is_live {
            problem_reported = false;
            continue;
        }
        let cfg = load_config(&state.db).await;
        let health = state.egress.health().await;
        if health.is_empty() { continue; } // Watch-page-only broadcast is valid.
        // A single unhealthy destination does not justify restarting healthy ones.
        let all_unhealthy = health.iter().all(|h| {
            !h.process_alive
                || (h.media_started && h.last_media_write_age_seconds
                    .is_some_and(|age| age >= cfg.stale_after_seconds))
                || (!h.media_started && h.running_for_seconds >= cfg.stale_after_seconds)
        });
        if !all_unhealthy {
            problem_reported = false;
            continue;
        }
        if !problem_reported {
            tracing::warn!("reliability: all configured FFmpeg outputs stalled or exited; enabled_auto_restart={}", cfg.automatic_restart);
            problem_reported = true;
        }
        if !cfg.automatic_restart { continue; }
        let now = Instant::now();
        if last_restart.is_some_and(|last| now.duration_since(last) < Duration::from_secs(cfg.cooldown_seconds)) {
            continue;
        }
        restart_history.retain(|t| now.duration_since(*t) < Duration::from_secs(1800));
        if restart_history.len() >= 2 {
            // Avoid an endless reconnect loop and preserve a diagnosable failure.
            continue;
        }
        let program = *state.program_source.borrow();
        let Some(source_id) = program else { continue; };
        if state.get_media_relay(&source_id).await.is_none() { continue; }
        tracing::warn!("reliability: retrying all stalled FFmpeg destinations (attempt {}/2 in 30 min)", restart_history.len() + 1);
        last_restart = Some(now);
        restart_history.push(now);
        crate::egress_restart_for(&state, source_id).await;
        problem_reported = false;
    }
}
