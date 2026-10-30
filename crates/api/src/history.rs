// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.
//! Durable, sampled local broadcast history. Does not assert YouTube remote health.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use axum::{extract::{Query, State}, http::{header, HeaderMap, StatusCode}, Json};
use chrono::{DateTime, SecondsFormat, Utc};
use muxshed_common::{PipelineState, SourceState};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;
use crate::{error::ApiError, state::AppState};

fn now_iso() -> String { Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true) }

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BroadcastSession {
    pub id: String,
    pub started_at: String,
    pub last_seen_at: String,
    pub ended_at: Option<String>,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HistoryEvent {
    pub id: i64,
    pub at_utc: String,
    pub event_type: String,
    pub subject: Option<String>,
    pub detail: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DailySample {
    pub day: String,
    pub samples: i64,
    pub broadcasting_samples: i64,
    pub encoder_observed_samples: i64,
    pub healthy_encoder_samples: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistorySummary {
    pub sessions: usize,
    pub observed_broadcast_seconds: i64,
    pub camera_outages: i64,
    pub recoveries: i64,
    pub failover_events: i64,
    pub ad_starts: i64,
    pub ad_ends: i64,
    pub encoder_alerts: i64,
    pub encoder_observed_samples: i64,
    pub encoder_healthy_samples: i64,
    pub estimated_local_encoder_health_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HistoryReport {
    pub days: i64,
    pub from_utc: String,
    pub to_utc: String,
    pub summary: HistorySummary,
    pub sessions: Vec<BroadcastSession>,
    pub events: Vec<HistoryEvent>,
    pub daily: Vec<DailySample>,
    pub retention_days: i64,
    pub notice: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct RangeQuery { pub days: Option<i64> }
fn days_or_default(query: RangeQuery) -> i64 {
    match query.days.unwrap_or(1) { 7 => 7, 30 => 30, _ => 1 }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistorySettings { pub retention_days: i64 }

pub async fn read_retention(db: &SqlitePool) -> i64 {
    let value: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM settings WHERE key = 'history_retention_days'"
    ).fetch_optional(db).await.ok().flatten();
    value.and_then(|(s,)| s.parse::<i64>().ok())
        .filter(|days| [30, 90, 180, 365].contains(days))
        .unwrap_or(90)
}

pub async fn settings(State(state): State<Arc<AppState>>) -> Json<HistorySettings> {
    Json(HistorySettings { retention_days: read_retention(&state.db).await })
}

pub async fn save_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<HistorySettings>,
) -> Result<Json<HistorySettings>, (StatusCode, String)> {
    if ![30, 90, 180, 365].contains(&body.retention_days) {
        return Err((StatusCode::BAD_REQUEST, "Retention must be 30, 90, 180 or 365 days".into()));
    }
    sqlx::query("INSERT INTO settings(key,value) VALUES('history_retention_days',?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(body.retention_days.to_string()).execute(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(body))
}

pub async fn record_event(
    db: &SqlitePool,
    event_type: &str,
    subject: Option<&str>,
    detail: Option<&str>,
    duration_ms: Option<i64>,
) {
    // Only internally-generated event metadata. Never persist RTMP URLs or keys.
    let session_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM history_sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1"
    ).fetch_optional(db).await.ok().flatten();
    if let Err(error) = sqlx::query(
        "INSERT INTO history_events(at_utc,session_id,event_type,subject,detail,duration_ms) VALUES(?,?,?,?,?,?)"
    )
        .bind(now_iso()).bind(session_id).bind(event_type)
        .bind(subject).bind(detail).bind(duration_ms)
        .execute(db).await
    { tracing::warn!("history: event write failed: {}", error); }
}

fn elapsed_seconds(start: &str, finish: &str, window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> i64 {
    let Ok(start) = DateTime::parse_from_rfc3339(start) else { return 0; };
    let Ok(end) = DateTime::parse_from_rfc3339(finish) else { return 0; };
    let from = start.with_timezone(&Utc).max(window_start);
    let to = end.with_timezone(&Utc).min(window_end);
    (to - from).num_seconds().max(0)
}

pub async fn report(
    State(state): State<Arc<AppState>>,
    Query(range): Query<RangeQuery>,
) -> Result<Json<HistoryReport>, ApiError> {
    let days = days_or_default(range);
    let now = Utc::now();
    let from = now - chrono::Duration::days(days);
    let from_s = from.to_rfc3339_opts(SecondsFormat::Millis, true);
    let to_s = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let all_sessions: Vec<BroadcastSession> = sqlx::query_as(
        "SELECT id,started_at,last_seen_at,ended_at,end_reason FROM history_sessions WHERE started_at < ? AND COALESCE(ended_at,last_seen_at) >= ? ORDER BY started_at DESC LIMIT 500"
    ).bind(&to_s).bind(&from_s).fetch_all(&state.db).await?;
    let events: Vec<HistoryEvent> = sqlx::query_as(
        "SELECT id,at_utc,event_type,subject,detail,duration_ms FROM history_events WHERE at_utc >= ? AND at_utc <= ? ORDER BY id DESC LIMIT 500"
    ).bind(&from_s).bind(&to_s).fetch_all(&state.db).await?;
    let daily: Vec<DailySample> = sqlx::query_as(
        "SELECT substr(observed_at,1,10) AS day, COUNT(*) AS samples, COALESCE(SUM(broadcast_live),0) AS broadcasting_samples, COALESCE(SUM(CASE WHEN broadcast_live=1 AND encoder_healthy IS NOT NULL THEN 1 ELSE 0 END),0) AS encoder_observed_samples, COALESCE(SUM(CASE WHEN broadcast_live=1 AND encoder_healthy=1 THEN 1 ELSE 0 END),0) AS healthy_encoder_samples FROM history_samples WHERE observed_at >= ? AND observed_at <= ? GROUP BY substr(observed_at,1,10) ORDER BY day"
    ).bind(&from_s).bind(&to_s).fetch_all(&state.db).await?;
    let observed_seconds = all_sessions.iter().map(|s| elapsed_seconds(
        &s.started_at, s.ended_at.as_deref().unwrap_or(&s.last_seen_at), from.clone(), now.clone()
    )).sum();
    // Counts are based on the bounded event list. For larger windows, query SQL counts independently.
    let mut counts = HashMap::<String, i64>::new();
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT event_type, COUNT(*) FROM history_events WHERE at_utc >= ? AND at_utc <= ? GROUP BY event_type"
    ).bind(&from_s).bind(&to_s).fetch_all(&state.db).await?;
    for (kind, count) in rows { counts.insert(kind, count); }
    let count = |kind: &str| -> i64 { counts.get(kind).copied().unwrap_or(0) };
    let encoder_observed: i64 = daily.iter().map(|x| x.encoder_observed_samples).sum();
    let encoder_healthy: i64 = daily.iter().map(|x| x.healthy_encoder_samples).sum();
    Ok(Json(HistoryReport {
        days, from_utc: from_s, to_utc: to_s,
        summary: HistorySummary {
            sessions: all_sessions.len(),
            observed_broadcast_seconds: observed_seconds,
            camera_outages: count("camera_offline"),
            recoveries: count("camera_recovered"),
            failover_events: count("failover_started"),
            ad_starts: count("ad_started"),
            ad_ends: count("ad_ended"),
            encoder_alerts: count("encoder_stalled"),
            encoder_observed_samples: encoder_observed,
            encoder_healthy_samples: encoder_healthy,
            estimated_local_encoder_health_percent: if encoder_observed > 0 {
                Some((encoder_healthy as f64 * 10000.0 / encoder_observed as f64).round() / 100.0)
            } else { None },
        },
        sessions: all_sessions, events, daily,
        retention_days: read_retention(&state.db).await,
        notice: "Local Muxshed observations sampled every 5s (events) / 30s (encoder). Historical data begins when this version first runs. Encoder health is not independently verified YouTube health; missing samples are not assumed healthy.",
    }))
}

fn csv_cell(value: &str) -> String {
    // Prevent spreadsheet formula execution on imported CSV text.
    let safe = if value.chars().next().is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r')) {
        format!("'{}", value)
    } else { value.to_string() };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

pub async fn export_events(
    State(state): State<Arc<AppState>>,
    Query(range): Query<RangeQuery>,
) -> Result<(HeaderMap, String), ApiError> {
    let days = days_or_default(range);
    let from = (Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let events: Vec<HistoryEvent> = sqlx::query_as(
        "SELECT id,at_utc,event_type,subject,detail,duration_ms FROM history_events WHERE at_utc >= ? ORDER BY id DESC LIMIT 50000"
    ).bind(&from).fetch_all(&state.db).await?;
    let mut output = String::from("timestamp_utc,event_type,subject,detail,duration_ms\r\n");
    for event in events {
        output.push_str(&format!("{},{},{},{},{}\r\n",
            csv_cell(&event.at_utc), csv_cell(&event.event_type),
            csv_cell(event.subject.as_deref().unwrap_or("")), csv_cell(event.detail.as_deref().unwrap_or("")),
            event.duration_ms.map(|v| v.to_string()).unwrap_or_default()));
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/csv; charset=utf-8".parse().unwrap());
    headers.insert(header::CONTENT_DISPOSITION, "attachment; filename=\"muxshed-events.csv\"".parse().unwrap());
    Ok((headers, output))
}

pub async fn export_sessions(
    State(state): State<Arc<AppState>>,
    Query(range): Query<RangeQuery>,
) -> Result<(HeaderMap, String), ApiError> {
    let days = days_or_default(range);
    let from = (Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let sessions: Vec<BroadcastSession> = sqlx::query_as(
        "SELECT id,started_at,last_seen_at,ended_at,end_reason FROM history_sessions WHERE COALESCE(ended_at,last_seen_at) >= ? ORDER BY started_at DESC LIMIT 50000"
    ).bind(&from).fetch_all(&state.db).await?;
    let mut output = String::from("session_id,started_at_utc,last_observed_at_utc,ended_at_utc,end_reason\r\n");
    for session in sessions {
        output.push_str(&format!("{},{},{},{},{}\r\n",
            csv_cell(&session.id), csv_cell(&session.started_at), csv_cell(&session.last_seen_at),
            csv_cell(session.ended_at.as_deref().unwrap_or("")), csv_cell(session.end_reason.as_deref().unwrap_or(""))));
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/csv; charset=utf-8".parse().unwrap());
    headers.insert(header::CONTENT_DISPOSITION, "attachment; filename=\"muxshed-sessions.csv\"".parse().unwrap());
    Ok((headers, output))
}

async fn close_previous_runs(db: &SqlitePool) -> Result<(), sqlx::Error> {
    // An unclean restart ends at the LAST OBSERVED heartbeat, not at this boot.
    sqlx::query("UPDATE history_sessions SET ended_at=last_seen_at, end_reason='restart_or_crash' WHERE ended_at IS NULL")
        .execute(db).await?;
    Ok(())
}

async fn cleanup(db: &SqlitePool) {
    let days = read_retention(db).await;
    let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Millis, true);
    for query in [
        "DELETE FROM history_events WHERE at_utc < ?",
        "DELETE FROM history_samples WHERE observed_at < ?",
        "DELETE FROM history_sessions WHERE ended_at IS NOT NULL AND ended_at < ?",
    ] {
        if let Err(error) = sqlx::query(query).bind(&cutoff).execute(db).await {
            tracing::warn!("history retention: {}", error);
        }
    }
}

/// Start only after migrations and AppState initialization. Never restarts encoders.
pub async fn run_history_monitor(state: Arc<AppState>) {
    if let Err(error) = close_previous_runs(&state.db).await {
        tracing::warn!("history: initial recovery failed: {}", error);
        return;
    }
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_cleanup = std::time::Instant::now() - Duration::from_secs(3600);
    let mut last_sample = std::time::Instant::now() - Duration::from_secs(30);
    let mut current_session: Option<String> = None;
    let mut sources_previous: HashMap<Uuid, bool> = HashMap::new();
    let mut offline_since: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
    let mut failover_previous: Option<bool> = None;
    let mut failover_since: Option<DateTime<Utc>> = None;
    let mut encoder_previous: Option<bool> = None;
    let mut encoder_since: Option<DateTime<Utc>> = None;
    loop {
        tick.tick().await;
        let now = now_iso();
        let timestamp = Utc::now();
        let broadcasting = matches!(state.pipeline.state().await, PipelineState::Live { .. });
        if broadcasting {
            if current_session.is_none() {
                let id = Uuid::new_v4().to_string();
                match sqlx::query("INSERT INTO history_sessions(id,started_at,last_seen_at) VALUES(?,?,?)")
                    .bind(&id).bind(&now).bind(&now).execute(&state.db).await {
                    Ok(_) => {
                        current_session = Some(id);
                        record_event(&state.db, "broadcast_started", None, None, None).await;
                    }
                    Err(e) => tracing::warn!("history: cannot start session: {}", e),
                }
            } else if let Some(ref session) = current_session {
                if let Err(e) = sqlx::query("UPDATE history_sessions SET last_seen_at=? WHERE id=? AND ended_at IS NULL")
                    .bind(&now).bind(session).execute(&state.db).await {
                    tracing::warn!("history: heartbeat failed: {}", e);
                }
            }
        } else if let Some(session) = current_session.take() {
            record_event(&state.db, "broadcast_stopped", None, None, None).await;
            let _ = sqlx::query("UPDATE history_sessions SET ended_at=?,last_seen_at=?,end_reason='broadcast_stopped' WHERE id=?")
                .bind(&now).bind(&now).bind(session).execute(&state.db).await;
            failover_previous = None;
            encoder_previous = None;
        }

        // Restrict camera health events to persistent sources. Temporary ad sources
        // and media file playout are not camera outages.
        let persistent: Vec<(String, String)> = match sqlx::query_as(
            "SELECT id,name FROM sources WHERE kind NOT LIKE '%\"media_file\"%'"
        ).fetch_all(&state.db).await {
            Ok(rows) => rows,
            Err(e) => { tracing::warn!("history: sources query failed: {}", e); Vec::new() }
        };
        let source_states = state.source_states.read().await.clone();
        let mut still_present = std::collections::HashSet::new();
        for (id, name) in persistent {
            let Ok(id) = Uuid::parse_str(&id) else { continue; };
            still_present.insert(id);
            let is_live = source_states.get(&id) == Some(&SourceState::Live);
            match sources_previous.insert(id, is_live) {
                Some(true) if !is_live => {
                    offline_since.insert(id, timestamp);
                    record_event(&state.db, "camera_offline", Some(&name), None, None).await;
                }
                Some(false) if is_live => {
                    let duration = offline_since.remove(&id)
                        .map(|since| (timestamp-since).num_milliseconds().max(0));
                    // Startup connection is an online transition, not a recovery
                    // from an outage observed by this monitor.
                    let kind = if duration.is_some() { "camera_recovered" } else { "camera_online" };
                    record_event(&state.db, kind, Some(&name), None, duration).await;
                }
                _ => {}
            }
        }
        sources_previous.retain(|id, _| still_present.contains(id));
        offline_since.retain(|id, _| still_present.contains(id));

        let failover = broadcasting && *state.failover_active.borrow();
        if let Some(prev) = failover_previous.replace(failover) {
            if !prev && failover {
                failover_since = Some(timestamp);
                record_event(&state.db, "failover_started", None, None, None).await;
            } else if prev && !failover {
                let elapsed = failover_since.take().map(|t| (timestamp-t).num_milliseconds().max(0));
                record_event(&state.db, "failover_ended", None, None, elapsed).await;
            }
        }
        let health = state.egress.health().await;
        let encoder_good = if broadcasting && !health.is_empty() {
            Some(health.iter().all(|h| h.process_alive && (
                (h.media_started && h.last_media_write_age_seconds.is_some_and(|age| age < 30))
                || (!h.media_started && h.running_for_seconds < 30)
            )))
        } else { None };
        if let Some(good) = encoder_good {
            if let Some(previous) = encoder_previous.replace(good) {
                if previous && !good {
                    encoder_since = Some(timestamp);
                    record_event(&state.db, "encoder_stalled", None, Some("local FFmpeg only"), None).await;
                } else if !previous && good {
                    let elapsed = encoder_since.take().map(|t| (timestamp-t).num_milliseconds().max(0));
                    record_event(&state.db, "encoder_recovered", None, Some("local FFmpeg only"), elapsed).await;
                }
            }
        } else { encoder_previous = None; encoder_since = None; }
        if last_sample.elapsed() >= Duration::from_secs(30) {
            last_sample = std::time::Instant::now();
            let selected_id = (*state.program_source.borrow()).map(|id| id.to_string());
            let _ = sqlx::query("INSERT INTO history_samples(observed_at,broadcast_live,program_source_id,failover_active,encoder_healthy,encoder_destinations) VALUES(?,?,?,?,?,?)")
                .bind(&now)
                .bind(if broadcasting {1} else {0})
                .bind(selected_id)
                .bind(if failover {1} else {0})
                .bind(encoder_good.map(|x| if x {1} else {0}))
                .bind(health.len() as i64)
                .execute(&state.db).await;
        }
        if last_cleanup.elapsed() >= Duration::from_secs(3600) {
            last_cleanup = std::time::Instant::now();
            cleanup(&state.db).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_excludes_nonmatching_time() {
        let now = DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z").unwrap().with_timezone(&Utc);
        let prior = now - chrono::Duration::hours(24);
        assert_eq!(elapsed_seconds("2026-10-29T11:00:00Z","2026-10-29T13:00:00Z",prior,now), 3600);
        assert_eq!(elapsed_seconds("2026-10-28T11:00:00Z","2026-10-28T13:00:00Z",prior,now), 0);
    }
    #[test]
    fn csv_formula_is_quoted_and_neutralized() {
        assert_eq!(csv_cell("=HYPERLINK(1)"), "\"'=HYPERLINK(1)\"");
        assert_eq!(csv_cell("text,here"), "\"text,here\"");
    }
}
