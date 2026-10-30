// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Persistent RTSP/IP-camera input. FFmpeg runs inside Muxshed, not in a
//! separately managed bridge container. When the camera/network drops,
//! the supervisor reconnects using the URL saved with the source.
//!
//! RTSP video must be H.264 for FLV stream-copy; audio is encoded as AAC.

use bytes::Bytes;
use muxshed_common::{SourceState, WsEvent};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::rtmp::flv;
use tokio::io::AsyncReadExt;
use tokio::process::{ChildStdout, Command};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::state::AppState;

/// Perform simple validation before persisting the URL. FFmpeg receives it as
/// one argument (never a shell command), so URL contents cannot become flags.
pub fn validate_rtsp_url(url: &str) -> Result<(), String> {
    if !url.starts_with("rtsp://") && !url.starts_with("rtsps://") {
        return Err("RTSP URL must start with rtsp:// or rtsps://".into());
    }
    if url.len() > 2048 || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("RTSP URL contains invalid characters or is too long".into());
    }
    let authority = url.split_once("://").map(|(_, rest)| rest.split('/').next().unwrap_or(""));
    if authority.unwrap_or("").is_empty() {
        return Err("RTSP URL must include the camera host".into());
    }
    Ok(())
}

async fn source_state(state: &AppState, id: Uuid, status: SourceState) {
    state.source_states.write().await.insert(id, status.clone());
    let _ = state.ws_tx.send(WsEvent::SourceState { id, state: status });
}

/// Start (or replace) the persistent supervisor for one saved RTSP source.
pub async fn start_rtsp_source(
    state: Arc<AppState>,
    source_id: Uuid,
    url: String,
) -> Result<(), String> {
    validate_rtsp_url(&url)?;
    // Stop the previous supervisor before inserting the replacement.
    let previous = { state.rtsp_sources.write().await.remove(&source_id) };
    if let Some(old) = previous {
        old.abort();
        let _ = old.await;
    }
    let relay = state.get_or_create_media_relay(source_id).await;
    source_state(&state, source_id, SourceState::Connecting).await;
    let worker_state = state.clone();
    let task = tokio::spawn(async move {
        // Keep the same source clock across successive FFmpeg restarts.
        let source_clock = Instant::now();
        loop {
            source_state(&worker_state, source_id, SourceState::Connecting).await;
            // Do not log the RTSP URL: it may contain camera credentials.
            // Copy H.264 without re-encoding; convert optional audio to AAC.
            let spawn_result = Command::new("ffmpeg")
                .args([
                    "-hide_banner", "-nostdin", "-loglevel", "error",
                    "-rtsp_transport", "tcp", "-rw_timeout", "5000000",
                    "-fflags", "+genpts", "-i", url.as_str(),
                    "-map", "0:v:0", "-map", "0:a:0?",
                    "-c:v", "copy", "-c:a", "aac", "-ar", "48000", "-ac", "2",
                    "-f", "flv", "-flvflags", "no_duration_filesize", "pipe:1",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                // FFmpeg logs sometimes print RTSP URLs (including passwords).
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn();

            match spawn_result {
                Ok(mut child) => {
                    if let Some(stdout) = child.stdout.take() {
                        let base_ms = source_clock.elapsed().as_millis().min(u32::MAX as u128) as u32;
                        let received = read_flv_output(
                            source_id, stdout, relay.clone(), worker_state.clone(), base_ms,
                        ).await;
                        if received == 0 {
                            tracing::warn!("rtsp: source {} received no usable FLV tags; retrying", source_id);
                        } else {
                            tracing::warn!("rtsp: source {} interrupted after {} tags; reconnecting", source_id, received);
                        }
                    }
                    // Terminate a stalled/ended worker before reconnecting.
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
                Err(e) => {
                    tracing::warn!("rtsp: source {} could not start FFmpeg: {}", source_id, e);
                }
            }
            worker_state.sequence_headers.write().await.remove(&source_id);
            source_state(&worker_state, source_id, SourceState::Disconnected).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    state.rtsp_sources.write().await.insert(source_id, task);
    Ok(())
}

async fn read_flv_output(
    source_id: Uuid,
    mut stdout: ChildStdout,
    relay: broadcast::Sender<Bytes>,
    state: Arc<AppState>,
    base_ms: u32,
) -> u64 {
    let mut header = [0u8; 13];
    if stdout.read_exact(&mut header).await.is_err() || &header[0..3] != b"FLV" {
        return 0;
    }

    let mut count = 0u64;
    let mut live = false;
    let mut first_input_ts: Option<u32> = None;
    loop {
        let mut tag_header = [0u8; 11];
        if stdout.read_exact(&mut tag_header).await.is_err() {
            break;
        }
        let tag_type = tag_header[0];
        let data_size = ((tag_header[1] as usize) << 16)
            | ((tag_header[2] as usize) << 8)
            | (tag_header[3] as usize);
        // Guard against damaged FLV and unlimited allocations.
        if data_size > 16 * 1024 * 1024 {
            tracing::warn!("rtsp: source {} oversized FLV tag", source_id);
            break;
        }
        let mut data = vec![0u8; data_size];
        if stdout.read_exact(&mut data).await.is_err() {
            break;
        }
        let mut previous_size = [0u8; 4];
        if stdout.read_exact(&mut previous_size).await.is_err() {
            break;
        }
        let mut tag = Vec::with_capacity(11 + data_size + 4);
        tag.extend_from_slice(&tag_header);
        tag.extend_from_slice(&data);
        tag.extend_from_slice(&previous_size);
        let original = Bytes::from(tag);
        // RTSP/FFmpeg starts its PTS clock over on every reconnect. Map each
        // connection onto one monotonic source timeline before program routing.
        let packet = if let Some(ts) = flv::read_tag_timestamp(&original) {
            let first = *first_input_ts.get_or_insert(ts);
            flv::rewrite_tag_timestamp(&original, base_ms.saturating_add(ts.saturating_sub(first)))
        } else {
            original
        };

        if tag_type == 9 && data.len() >= 2 {
            let avc_packet_type = data[1];
            let frame_type = (data[0] >> 4) & 0x0f;
            if avc_packet_type == 0 {
                state.sequence_headers.write().await.entry(source_id)
                    .or_default().video = Some(packet.clone());
            } else if avc_packet_type == 1 && frame_type == 1 {
                state.sequence_headers.write().await.entry(source_id)
                    .or_default().last_keyframe = Some(packet.clone());
                if !live {
                    live = true;
                    source_state(&state, source_id, SourceState::Live).await;
                    tracing::info!("rtsp: source {} is live", source_id);
                }
            }
        } else if tag_type == 8 && data.len() >= 2 && data[1] == 0 {
            state.sequence_headers.write().await.entry(source_id)
                .or_default().audio = Some(packet.clone());
        }
        let _ = relay.send(packet);
        count += 1;
    }
    count
}

/// Stop the persistent source (on delete or source-type change).
pub async fn stop_rtsp_source(state: &AppState, source_id: &Uuid) {
    let previous = { state.rtsp_sources.write().await.remove(source_id) };
    if let Some(task) = previous {
        task.abort();
        let _ = task.await;
    }
    state.remove_media_relay(source_id).await;
    state.sequence_headers.write().await.remove(source_id);
    source_state(state, *source_id, SourceState::Disconnected).await;
}
