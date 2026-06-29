// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Program failover supervisor.
//!
//! Sits between the operator's program selection (`program_intent`) and the
//! source the program router actually forwards (`program_source`). While the
//! intended source produces video it mirrors intent -> source. When the intended
//! source stops producing video for `offline_grace_ms` it substitutes the
//! configured fallback source so destinations keep receiving a stream, then
//! restores the intended source once it has been producing video again for
//! `recovery_grace_ms`.

use crate::state::AppState;
use bytes::Bytes;
use muxshed_common::{FailoverConfig, SourceKind, WsEvent};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

const FLV_TAG_VIDEO: u8 = 9;

fn is_video_tag(data: &[u8]) -> bool {
    !data.is_empty() && data[0] == FLV_TAG_VIDEO
}

/// Load the persisted failover config (defaults = disabled).
pub async fn load_config(state: &AppState) -> FailoverConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'failover_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(json,)| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn route(state: &AppState, source: Option<Uuid>) {
    let _ = state.program_source.send(source);
}

pub async fn run_failover_supervisor(state: Arc<AppState>) {
    let mut intent_rx = state.program_intent.subscribe();
    loop {
        let intent = *intent_rx.borrow_and_update();
        let cfg = load_config(&state).await;
        let fallback = cfg.fallback_source_id;

        // Passthrough: failover off, no fallback, nothing selected, or the
        // fallback IS the selection. Just mirror intent -> routed source.
        if !cfg.enabled || fallback.is_none() || intent.is_none() || fallback == intent {
            set_failover(&state, false, fallback, intent).await;
            route(&state, intent);
            if intent_rx.changed().await.is_err() {
                return;
            }
            continue;
        }

        let intended = intent.unwrap();
        let fallback = fallback.unwrap();
        ensure_fallback_running(&state, fallback).await;

        // Monitor until the operator changes intent (or a config change nudges
        // program_intent), then re-evaluate from the top.
        monitor(&state, &mut intent_rx, intended, fallback, &cfg).await;
    }
}

async fn monitor(
    state: &Arc<AppState>,
    intent_rx: &mut watch::Receiver<Option<Uuid>>,
    intended: Uuid,
    fallback: Uuid,
    cfg: &FailoverConfig,
) {
    let offline = Duration::from_millis(cfg.offline_grace_ms.max(250));
    let recovery = Duration::from_millis(cfg.recovery_grace_ms.max(250));

    let mut rx: Option<broadcast::Receiver<Bytes>> =
        state.get_media_relay(&intended).await.map(|t| t.subscribe());
    let mut last_video = Instant::now();
    let mut recovering_since: Option<Instant> = None;
    let mut showing_fallback = false;
    // Whether the intended source has ever produced a video frame. Before its
    // first frame it may just be warming up (an RTMP source's normalizer takes a
    // second or two), so give it a longer startup grace rather than failing over
    // on a source that simply hasn't started yet.
    let mut ever_live = false;
    let startup_grace = offline.max(Duration::from_secs(5));

    route(state, Some(intended));
    set_failover(state, false, Some(fallback), Some(intended)).await;

    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Operator switched (or a config change nudged intent) — bail to re-evaluate.
            r = intent_rx.changed() => {
                let _ = r;
                return;
            }
            // Frames from the intended source.
            msg = async {
                match &mut rx {
                    Some(r) => r.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match msg {
                    Ok(data) => {
                        if is_video_tag(&data) {
                            last_video = Instant::now();
                            ever_live = true;
                            if showing_fallback {
                                recovering_since.get_or_insert_with(Instant::now);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => rx = None,
                }
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                // Re-subscribe if the source reconnected (a fresh relay).
                if rx.is_none() {
                    if let Some(t) = state.get_media_relay(&intended).await {
                        rx = Some(t.subscribe());
                    }
                }
                let dark_for = now.duration_since(last_video);
                let threshold = if ever_live { offline } else { startup_grace };

                if !showing_fallback && dark_for > threshold {
                    tracing::warn!(
                        "failover: program source {} offline ({}ms) -> fallback {}",
                        intended, dark_for.as_millis(), fallback
                    );
                    ensure_fallback_running(state, fallback).await;
                    route(state, Some(fallback));
                    restart_egress(state, fallback).await;
                    showing_fallback = true;
                    recovering_since = None;
                    set_failover(state, true, Some(fallback), Some(intended)).await;
                } else if showing_fallback {
                    if dark_for > Duration::from_millis(750) {
                        // Intended went dark again before recovery confirmed.
                        recovering_since = None;
                    } else if let Some(since) = recovering_since {
                        if now.duration_since(since) > recovery {
                            tracing::info!("failover: program source {} recovered -> restoring", intended);
                            route(state, Some(intended));
                            restart_egress(state, intended).await;
                            showing_fallback = false;
                            recovering_since = None;
                            set_failover(state, false, Some(fallback), Some(intended)).await;
                        }
                    }
                }
            }
        }
    }
}

/// When live, restart the egress with a fresh encoder so the destination gets a
/// clean stream across the program splice (a single long-running encoder chokes
/// on a mid-stream source change). No-op in studio mode (egress not running).
async fn restart_egress(state: &AppState, source: Uuid) {
    if state.egress.is_running().await {
        let seq = state.sequence_headers.read().await.get(&source).cloned();
        state.egress.restart(seq).await;
    }
}

async fn set_failover(state: &AppState, active: bool, fallback: Option<Uuid>, intent: Option<Uuid>) {
    let changed = *state.failover_active.borrow() != active;
    let _ = state.failover_active.send(active);
    if changed {
        let _ = state.ws_tx.send(WsEvent::FailoverState {
            active,
            fallback_source_id: fallback,
            intent_source_id: intent,
        });
    }
}

/// Pre-start an image/video fallback so its relay has frames the instant we cut.
/// A live-ingress fallback already has a relay when its encoder connects.
async fn ensure_fallback_running(state: &Arc<AppState>, fallback: Uuid) {
    if state.get_media_relay(&fallback).await.is_some() {
        return;
    }
    let row = sqlx::query_as::<_, (String,)>("SELECT kind FROM sources WHERE id = ?")
        .bind(fallback.to_string())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some((kind_json,)) = row {
        if let Ok(SourceKind::MediaFile { file_path, loop_mode, .. }) =
            serde_json::from_str::<SourceKind>(&kind_json)
        {
            let _ = crate::media_player::start_media_playback(
                state.clone(),
                fallback,
                &file_path,
                &loop_mode,
            )
            .await;
        }
    }
}
