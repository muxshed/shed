// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Scheduler supervisor. Fires each enabled schedule when its persisted
//! `next_run_at` (computed in the system timezone, DST-correct) comes due, then
//! advances it. Skips with a notification if a broadcast is already live.

use crate::schedule_time::{next_run_after, parse_tz};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use muxshed_common::{TriggerKind, WsEvent};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub async fn run_scheduler(state: Arc<AppState>) {
    let mut nudge = state.schedule_nudge.subscribe();
    loop {
        ensure_next_runs(&state).await;
        let now = Utc::now();

        for (id, trigger) in due_schedules(&state, now).await {
            handle_due(&state, id, &trigger).await;
        }

        let sleep_for = soonest_next_run(&state).await
            .map(|t| (t - Utc::now()).to_std().unwrap_or(Duration::from_secs(0)))
            .unwrap_or(Duration::from_secs(60))
            .clamp(Duration::from_millis(200), Duration::from_secs(60));

        tokio::select! {
            _ = tokio::time::sleep(sleep_for) => {}
            r = nudge.changed() => { if r.is_err() { return; } }
        }
    }
}

async fn handle_due(state: &Arc<AppState>, id: Uuid, trigger: &TriggerKind) {
    let live = state.active_schedule.borrow().is_some()
        || matches!(state.pipeline.state().await, muxshed_common::PipelineState::Live { .. });
    if live {
        tracing::info!("scheduler: skipping {} — already live", id);
        let _ = sqlx::query("INSERT INTO schedule_runs (id, schedule_id, status) VALUES (?, ?, 'skipped')")
            .bind(Uuid::new_v4().to_string()).bind(id.to_string()).execute(&state.db).await;
        let _ = state.ws_tx.send(WsEvent::ScheduleSkipped { id, reason: "already live".into() });
    } else {
        tracing::info!("scheduler: firing schedule {}", id);
        crate::playout::start_broadcast(state.clone(), id).await;
    }
    advance(state, id, trigger).await;
}

/// Move a schedule past the occurrence just handled: one-offs are disabled;
/// recurring schedules get their next future `next_run_at`.
async fn advance(state: &Arc<AppState>, id: Uuid, trigger: &TriggerKind) {
    match trigger {
        TriggerKind::Once { .. } => {
            let _ = sqlx::query("UPDATE schedules SET enabled = 0, next_run_at = NULL WHERE id = ?")
                .bind(id.to_string()).execute(&state.db).await;
        }
        TriggerKind::Cron { .. } => {
            let tz = parse_tz(&load_system_tz(state).await);
            let next = next_run_after(trigger, tz, Utc::now()).map(|d| d.to_rfc3339());
            let _ = sqlx::query("UPDATE schedules SET next_run_at = ? WHERE id = ?")
                .bind(next).bind(id.to_string()).execute(&state.db).await;
        }
    }
}

/// Enabled schedules whose next_run_at is due (<= now).
async fn due_schedules(state: &AppState, now: DateTime<Utc>) -> Vec<(Uuid, TriggerKind)> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT id, trigger_kind, trigger_at, trigger_cron FROM schedules \
         WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?",
    ).bind(now.to_rfc3339()).fetch_all(&state.db).await.unwrap_or_default();
    rows.into_iter().filter_map(|(id, kind, at, cron)| {
        let sid: Uuid = id.parse().ok()?;
        let trigger = if kind == "cron" { TriggerKind::Cron { expr: cron.unwrap_or_default() } }
                      else { TriggerKind::Once { at: at.unwrap_or_default() } };
        Some((sid, trigger))
    }).collect()
}

/// Backfill next_run_at for enabled schedules without one; mark past one-offs missed.
async fn ensure_next_runs(state: &AppState) {
    let tz = parse_tz(&load_system_tz(state).await);
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT id, trigger_kind, trigger_at, trigger_cron FROM schedules WHERE enabled = 1 AND next_run_at IS NULL",
    ).fetch_all(&state.db).await.unwrap_or_default();
    for (id, kind, at, cron) in rows {
        let trigger = if kind == "cron" { TriggerKind::Cron { expr: cron.unwrap_or_default() } }
                      else { TriggerKind::Once { at: at.unwrap_or_default() } };
        if let Some(next) = next_run_after(&trigger, tz, Utc::now()) {
            let _ = sqlx::query("UPDATE schedules SET next_run_at = ? WHERE id = ?")
                .bind(next.to_rfc3339()).bind(&id).execute(&state.db).await;
        } else if matches!(trigger, TriggerKind::Once { .. }) {
            let _ = sqlx::query("UPDATE schedules SET enabled = 0 WHERE id = ?").bind(&id).execute(&state.db).await;
            let _ = sqlx::query("INSERT INTO schedule_runs (id, schedule_id, status) VALUES (?, ?, 'missed')")
                .bind(Uuid::new_v4().to_string()).bind(&id).execute(&state.db).await;
        }
    }
}

async fn soonest_next_run(state: &AppState) -> Option<DateTime<Utc>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT next_run_at FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL ORDER BY next_run_at ASC LIMIT 1")
        .fetch_optional(&state.db).await.ok().flatten();
    row.and_then(|(s,)| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc))
}

async fn load_system_tz(state: &AppState) -> String {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'system_timezone'")
        .fetch_optional(&state.db).await.ok().flatten().map(|r| r.0).unwrap_or_else(|| "UTC".into())
}
