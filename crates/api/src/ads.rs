// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Server-managed, one-shot commercial playout. The caller can close Studio:
//! playback completion and return-to-live are handled here, not in the browser.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;
use muxshed_common::{MuxshedError, PipelineState, SourceState};
use crate::error::ApiError;
use crate::state::{AppState, AudioRouting};

#[derive(Clone)]
pub struct AdSession {
    pub asset_id: String,
    pub ad_source: Uuid,
    pub previous_intent: Option<Uuid>,
    pub previous_audio: AudioRouting,
    pub started_at: Instant,
}

#[derive(Deserialize)]
pub struct PlayAdRequest {
    pub asset_id: String,
}

#[derive(Serialize)]
pub struct AdStatus {
    pub playing: bool,
    pub asset_id: Option<String>,
    pub source_id: Option<Uuid>,
    pub return_to: Option<Uuid>,
}

fn status_from(session: Option<&AdSession>) -> AdStatus {
    AdStatus {
        playing: session.is_some(),
        asset_id: session.map(|s| s.asset_id.clone()),
        source_id: session.map(|s| s.ad_source),
        return_to: session.and_then(|s| s.previous_intent),
    }
}

pub async fn status(State(state): State<Arc<AppState>>) -> Json<AdStatus> {
    let guard = state.ad_session.lock().await;
    Json(status_from(guard.as_ref()))
}

/// Start one video asset without looping; capture the current program source
/// and audio route so it is restored on EOF. Do not alter the broadcast's
/// destination/encoder or restart the live camera.
pub async fn play(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PlayAdRequest>,
) -> Result<Json<AdStatus>, ApiError> {
    if !matches!(state.pipeline.state().await, PipelineState::Live { .. }) {
        return Err(MuxshedError::BadRequest("start broadcasting before playing a commercial".into()).into());
    }
    let asset_id = Uuid::parse_str(&request.asset_id)
        .map_err(|_| MuxshedError::BadRequest("invalid asset id".into()))?;
    let asset = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT asset_type, file_path, duration_ms FROM assets WHERE id = ?",
    )
    .bind(asset_id.to_string())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound("commercial asset not found".into()))?;

    if asset.0 != "video" {
        return Err(MuxshedError::BadRequest("choose a video commercial from Library".into()).into());
    }
    let path = PathBuf::from(asset.1);
    if !path.is_file() {
        return Err(MuxshedError::BadRequest("commercial video file is missing".into()).into());
    }
    let current = *state.program_intent.borrow();
    if current.is_none() {
        return Err(MuxshedError::BadRequest("select a live camera or scene before playing a commercial".into()).into());
    }

    let source = Uuid::new_v4();
    let old_audio = state.audio_routing.borrow().clone();
    let session = AdSession {
        asset_id: asset_id.to_string(),
        ad_source: source,
        previous_intent: current,
        previous_audio: old_audio,
        started_at: Instant::now(),
    };
    {
        let mut guard = state.ad_session.lock().await;
        if guard.is_some() {
            return Err(MuxshedError::BadRequest("a commercial is already playing".into()).into());
        }
        *guard = Some(session.clone());
    }

    // Set up the source before routing to it. The short delay lets the router
    // subscribe before FFmpeg emits the first keyframe of the commercial.
    state.get_or_create_media_relay(source).await;
    state.source_states.write().await.insert(source, SourceState::Live);
    state.audio_routing.send_modify(|routing| {
        routing.active_audio_source = None;
        routing.audio_follows_video = true;
    });
    let _ = state.program_intent.send(Some(source));
    sleep(Duration::from_millis(350)).await;

    // A manual operator switch during warm-up cancels this ad.
    let still_selected = *state.program_intent.borrow() == Some(source);
    if !still_selected {
        finish(state.clone(), source).await;
        return Err(MuxshedError::BadRequest("commercial cancelled by an operator switch".into()).into());
    }

    if let Err(error) = crate::media_player::start_media_playback(state.clone(), source, &path, "one_shot").await {
        finish(state.clone(), source).await;
        return Err(MuxshedError::Internal(format!("could not play commercial: {}", error)).into());
    }

    tracing::info!("commercial started: asset={} source={}", asset_id, source);
    crate::history::record_event(&state.db, "ad_started", Some(&asset_id.to_string()), None, None).await;
    let monitor = state.clone();
    let max_duration = Duration::from_secs((asset.2.max(0) as u64 / 1000).max(30).min(3600) + 30);
    tokio::spawn(async move {
        let deadline = Instant::now() + max_duration;
        loop {
            sleep(Duration::from_millis(250)).await;
            if !monitor.media_players.read().await.contains_key(&source)
                || *monitor.program_intent.borrow() != Some(source)
                || Instant::now() >= deadline
            {
                break;
            }
        }
        finish(monitor, source).await;
    });
    Ok(Json(status_from(Some(&session))))
}

/// Return automatically on EOF, error, or explicit Stop Ad. Never overwrite a
/// deliberate source switch made by the operator during the commercial.
async fn finish(state: Arc<AppState>, source: Uuid) {
    let session = {
        let mut guard = state.ad_session.lock().await;
        if guard.as_ref().map(|s| s.ad_source) == Some(source) {
            guard.take()
        } else {
            None
        }
    };
    if let Some(session) = session {
        let still_selected = *state.program_intent.borrow() == Some(source);
        if still_selected {
            let _ = state.program_intent.send(session.previous_intent);
        }
        // Restore the previous audio route only if nobody changed the ad's
        // temporary follows-video audio routing in the meantime.
        let audio = state.audio_routing.borrow().clone();
        if audio.audio_follows_video && audio.active_audio_source.is_none() {
            let _ = state.audio_routing.send(session.previous_audio);
        }
        crate::media_player::stop_media_playback(&state, &source).await;
        state.source_states.write().await.remove(&source);
        tracing::info!("commercial finished: asset={} returned to {:?}", session.asset_id, session.previous_intent);
        // Playback was removed from media_players or explicitly stopped; the
        // monitor cannot always tell clean EOF from FFmpeg failure, so report
        // "ended" rather than claiming a verified completion.
        let elapsed_ms = session.started_at.elapsed().as_millis().min(i64::MAX as u128) as i64;
        crate::history::record_event(&state.db, "ad_ended", Some(&session.asset_id),
            Some("playback ended / return attempted"), Some(elapsed_ms)).await;
    }
}

pub async fn stop(State(state): State<Arc<AppState>>) -> Result<Json<AdStatus>, ApiError> {
    let source = state.ad_session.lock().await.as_ref().map(|s| s.ad_source);
    if let Some(id) = source {
        finish(state.clone(), id).await;
    }
    Ok(Json(status_from(state.ad_session.lock().await.as_ref())))
}
