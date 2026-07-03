// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Privileged management endpoints for the managed-hosting portal. Gated by the
//! management token (see `crate::auth::management_auth`), separate from tenant API
//! keys, and only mounted when a management token is configured. Not part of the
//! public OpenAPI.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::state::AppState;
use muxshed_common::SourceState;

/// Process start time, set in `main` at startup, for uptime reporting.
pub static START_TIME: OnceLock<Instant> = OnceLock::new();

fn uptime_secs() -> u64 {
    START_TIME.get().map(|s| s.elapsed().as_secs()).unwrap_or(0)
}

/// Best-effort process/system stats from /proc (Linux). Null on other platforms.
fn system_stats() -> Value {
    let rss_kb = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
        });
    let load_1m = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()));
    json!({ "process_rss_kb": rss_kb, "load_avg_1m": load_1m })
}

/// Health: version, uptime, pipeline state, headless flag.
pub async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    let pipeline = serde_json::to_value(state.pipeline.state().await).ok();
    let headless = state.config.read().await.headless;
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": uptime_secs(),
        "pipeline": pipeline,
        "headless": headless,
    }))
}

/// Stats: sources, destinations, egress throughput, recording, and system load.
pub async fn stats(State(state): State<Arc<AppState>>) -> Json<Value> {
    let (total_sources, live_sources) = {
        let s = state.source_states.read().await;
        (
            s.len(),
            s.values().filter(|v| matches!(v, SourceState::Live)).count(),
        )
    };
    let dest_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM destinations")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let dest_enabled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM destinations WHERE enabled = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let egress = state.egress.stats().await;

    Json(json!({
        "sources": { "total": total_sources, "live": live_sources },
        "destinations": { "total": dest_total, "enabled": dest_enabled },
        "egress": {
            "running": state.egress.is_running().await,
            "bytes_sent": egress.bytes_sent,
            "duration_secs": egress.duration_secs,
            "source_bitrate_kbps": egress.source_bitrate_kbps,
        },
        "recording": serde_json::to_value(state.pipeline.recording_state().await).ok(),
        "active_schedule": state.active_schedule.borrow().map(|id| id.to_string()),
        "system": system_stats(),
    }))
}

/// Connectivity: ingest ports, whether a source is live, program and egress state.
pub async fn connectivity(State(state): State<Arc<AppState>>) -> Json<Value> {
    let (rtmp_port, srt_start) = {
        let c = state.config.read().await;
        (c.rtmp_port, c.srt_port_range_start)
    };
    let source_live = {
        let s = state.source_states.read().await;
        s.values().any(|v| matches!(v, SourceState::Live))
    };
    Json(json!({
        "rtmp_port": rtmp_port,
        "srt_port_start": srt_start,
        "source_live": source_live,
        "program_on_air": state.program_source.borrow().is_some(),
        "egress_running": state.egress.is_running().await,
        "failover_active": *state.failover_active.borrow(),
    }))
}

/// Restart: exit the process so the supervisor (Docker, systemd) restarts it.
pub async fn restart() -> (StatusCode, Json<Value>) {
    tracing::warn!("admin: restart requested; exiting for the supervisor to restart");
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        std::process::exit(0);
    });
    (StatusCode::ACCEPTED, Json(json!({ "status": "restarting" })))
}

/// Read the effective (non-secret) config plus the stored settings.
pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    let c = state.config.read().await;
    let settings: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let settings_map: serde_json::Map<String, Value> = settings
        .into_iter()
        .map(|(k, v)| {
            let parsed = serde_json::from_str(&v).unwrap_or(Value::String(v));
            (k, parsed)
        })
        .collect();
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "headless": c.headless,
        "listen_addr": c.listen_addr,
        "rtmp_port": c.rtmp_port,
        "srt_port_start": c.srt_port_range_start,
        "data_dir": c.data_dir.display().to_string(),
        "settings": settings_map,
    }))
}

#[derive(serde::Deserialize)]
pub struct ConfigUpdate {
    pub settings: HashMap<String, Value>,
}

/// Push settings from the portal. Upserts each key into the settings table.
pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigUpdate>,
) -> Result<Json<Value>, StatusCode> {
    for (k, v) in &body.settings {
        let val = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind(k)
            .bind(&val)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(Json(json!({ "updated": body.settings.len() })))
}
