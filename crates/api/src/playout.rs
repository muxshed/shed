// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Playout controller: airs a schedule's content to program + destinations.
//! Phase 1 = a single VOD with a standby card around it and duration-based end.

use crate::state::AppState;
use muxshed_common::{DestinationKind, EndBehavior, WsEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

struct VodItem {
    asset_id: Uuid,
    file_path: PathBuf,
    duration_ms: u64,
}

/// Air the schedule now. Spawns a task that runs the broadcast and returns.
pub async fn start_broadcast(state: Arc<AppState>, schedule_id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = run_broadcast(&state, schedule_id).await {
            tracing::warn!("playout: broadcast {} error: {}", schedule_id, e);
            record_run(&state, schedule_id, "error").await;
            let _ = state.active_schedule.send(None);
        }
    });
}

async fn run_broadcast(state: &Arc<AppState>, schedule_id: Uuid) -> Result<(), String> {
    let vod = load_first_vod(state, schedule_id).await?;
    let end_behavior = load_end_behavior(state, schedule_id).await;
    let standby_asset = load_standby_asset(state, schedule_id).await;
    let destinations = load_destinations(state, schedule_id).await;

    let _ = state.active_schedule.send(Some(schedule_id));
    record_run(state, schedule_id, "ran").await;
    let _ = state.ws_tx.send(WsEvent::ScheduleStarted { id: schedule_id });
    tracing::info!("playout: schedule {} on air (vod {})", schedule_id, vod.asset_id);

    let standby_id = ensure_standby(state, &standby_asset).await;
    let cfg = load_output_config(state).await;
    let seq = if let Some(sid) = standby_id {
        state.sequence_headers.read().await.get(&sid).cloned()
    } else {
        None
    };
    if let Err(e) = state
        .egress
        .start(schedule_id, destinations, state.program_tx.clone(), Some(cfg), seq)
        .await
    {
        tracing::warn!("playout: egress start error (continuing): {}", e);
    }
    if let Some(sid) = standby_id {
        let _ = state.program_intent.send(Some(sid));
    }
    state.pipeline.start(Vec::new()).await.ok();

    let vod_source = Uuid::new_v4();
    let loop_mode = if matches!(end_behavior, EndBehavior::Loop) {
        "loop"
    } else {
        "one_shot"
    };
    crate::media_player::start_media_playback(state.clone(), vod_source, &vod.file_path, loop_mode)
        .await
        .map_err(|e| format!("vod playback: {}", e))?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = state.program_intent.send(Some(vod_source));
    crate::egress_restart_for(state, vod_source).await;

    if !matches!(end_behavior, EndBehavior::Loop) {
        let dur = Duration::from_millis(vod.duration_ms.max(1000));
        tokio::select! {
            _ = tokio::time::sleep(dur) => {}
            _ = wait_until_deactivated(state, schedule_id) => {
                crate::media_player::stop_media_playback(state, &vod_source).await;
                return Ok(());
            }
        }
        crate::media_player::stop_media_playback(state, &vod_source).await;
        match end_behavior {
            EndBehavior::Standby => {
                if let Some(sid) = standby_id {
                    let _ = state.program_intent.send(Some(sid));
                    crate::egress_restart_for(state, sid).await;
                }
            }
            _ => {
                state.egress.stop().await;
                state.pipeline.stop().await.ok();
                let _ = state.program_intent.send(None);
                if let Some(sid) = standby_id {
                    crate::media_player::stop_media_playback(state, &sid).await;
                }
                let _ = state.active_schedule.send(None);
                let _ = state.ws_tx.send(WsEvent::ScheduleEnded { id: schedule_id });
                mark_run_ended(state, schedule_id).await;
            }
        }
    }
    Ok(())
}

/// Stop the currently-airing broadcast (manual stop).
pub async fn stop_broadcast(state: &AppState) {
    let id = *state.active_schedule.borrow();
    state.egress.stop().await;
    state.pipeline.stop().await.ok();
    let _ = state.program_intent.send(None);
    let _ = state.active_schedule.send(None);
    if let Some(id) = id {
        let _ = state.ws_tx.send(WsEvent::ScheduleEnded { id });
        mark_run_ended(state, id).await;
    }
}

async fn wait_until_deactivated(state: &Arc<AppState>, schedule_id: Uuid) {
    let mut rx = state.active_schedule.subscribe();
    loop {
        if *rx.borrow_and_update() != Some(schedule_id) {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

async fn ensure_standby(state: &Arc<AppState>, standby: &Option<VodItem>) -> Option<Uuid> {
    let s = standby.as_ref()?;
    let id = Uuid::new_v4();
    let _ = crate::media_player::start_media_playback(state.clone(), id, &s.file_path, "loop").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    Some(id)
}

async fn load_first_vod(state: &AppState, schedule_id: Uuid) -> Result<VodItem, String> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT ref_id FROM schedule_items WHERE schedule_id = ? AND item_kind = 'vod' ORDER BY position ASC LIMIT 1",
    )
    .bind(schedule_id.to_string())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or("schedule has no VOD item")?;
    let asset_id: Uuid = row.0.parse().map_err(|_| "bad asset id")?;
    let a = sqlx::query_as::<_, (String, i64)>("SELECT file_path, duration_ms FROM assets WHERE id = ?")
        .bind(asset_id.to_string())
        .fetch_optional(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("VOD asset not found")?;
    Ok(VodItem {
        asset_id,
        file_path: PathBuf::from(a.0),
        duration_ms: a.1.max(0) as u64,
    })
}

async fn load_standby_asset(state: &AppState, schedule_id: Uuid) -> Option<VodItem> {
    let sid: Option<(Option<String>,)> =
        sqlx::query_as("SELECT standby_asset_id FROM schedules WHERE id = ?")
            .bind(schedule_id.to_string())
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let asset_id = sid.and_then(|r| r.0)?;
    let a = sqlx::query_as::<_, (String, i64)>("SELECT file_path, duration_ms FROM assets WHERE id = ?")
        .bind(&asset_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()?;
    Some(VodItem {
        asset_id: asset_id.parse().ok()?,
        file_path: PathBuf::from(a.0),
        duration_ms: a.1.max(0) as u64,
    })
}

async fn load_end_behavior(state: &AppState, schedule_id: Uuid) -> EndBehavior {
    sqlx::query_as::<_, (String,)>("SELECT end_behavior FROM schedules WHERE id = ?")
        .bind(schedule_id.to_string())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|r| EndBehavior::from_db(&r.0))
        .unwrap_or_default()
}

async fn load_destinations(state: &AppState, schedule_id: Uuid) -> Vec<muxshed_common::Destination> {
    let row: Option<(String,)> = sqlx::query_as("SELECT destination_ids FROM schedules WHERE id = ?")
        .bind(schedule_id.to_string())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let ids: Vec<String> = row.and_then(|r| serde_json::from_str(&r.0).ok()).unwrap_or_default();
    let mut out = Vec::new();
    for id in ids {
        if let Ok(Some(d)) = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT id, name, kind, enabled FROM destinations WHERE id = ?",
        )
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        {
            if let Ok(kind) = serde_json::from_str::<DestinationKind>(&d.2) {
                out.push(muxshed_common::Destination {
                    id: d.0.parse().unwrap_or_default(),
                    name: d.1,
                    kind,
                    enabled: d.3 != 0,
                });
            }
        }
    }
    out
}

async fn load_output_config(state: &AppState) -> crate::routes::output::OutputConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(j,)| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

async fn record_run(state: &AppState, schedule_id: Uuid, status: &str) {
    let _ = sqlx::query("INSERT INTO schedule_runs (id, schedule_id, started_at, status) VALUES (?, ?, ?, ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(schedule_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(status)
        .execute(&state.db)
        .await;
}

async fn mark_run_ended(state: &AppState, schedule_id: Uuid) {
    let _ = sqlx::query("UPDATE schedule_runs SET ended_at = ? WHERE schedule_id = ? AND ended_at IS NULL")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(schedule_id.to_string())
        .execute(&state.db)
        .await;
}
