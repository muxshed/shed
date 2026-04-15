// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::rtmp::flv;
use crate::state::AppState;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

const FLV_TAG_AUDIO: u8 = 8;
const FLV_TAG_VIDEO: u8 = 9;

fn tag_type(data: &[u8]) -> Option<u8> {
    if data.is_empty() {
        return None;
    }
    Some(data[0])
}

fn is_video_keyframe(data: &[u8]) -> bool {
    if data.len() < 13 || data[0] != FLV_TAG_VIDEO {
        return false;
    }
    let frame_type = data[11] & 0xF0;
    let is_seq_header = data[12] == 0x00;
    frame_type == 0x10 && !is_seq_header
}

fn is_sequence_header(data: &[u8]) -> bool {
    if data.len() < 13 {
        return false;
    }
    let tt = data[0];
    if tt == FLV_TAG_VIDEO {
        return data[12] == 0x00;
    }
    if tt == FLV_TAG_AUDIO && data.len() > 12 {
        return data[12] == 0x00;
    }
    false
}

/// Remap a tag's timestamp relative to a base, outputting a continuous timestamp.
fn remap_tag(data: &[u8], first_ts: &mut Option<u32>, base_output: u32, output_ts: &mut u32) -> Bytes {
    if let Some(src_ts) = flv::read_tag_timestamp(data) {
        let base = first_ts.get_or_insert(src_ts);
        let elapsed = src_ts.wrapping_sub(*base);
        let new_ts = base_output.wrapping_add(elapsed);
        *output_ts = new_ts;
        flv::rewrite_tag_timestamp(data, new_ts)
    } else {
        Bytes::copy_from_slice(data)
    }
}

/// Runs the program router: takes video from program_source, audio from audio routing.
/// Guarantees a gapless output stream by keeping the old source running until
/// the new source produces a keyframe.
pub async fn run_program_router(state: Arc<AppState>) {
    let mut source_rx = state.program_source.subscribe();
    let mut audio_routing_rx = state.audio_routing.subscribe();
    let mut output_ts: u32 = 0;

    loop {
        // Wait for a video source
        let current_source_id = loop {
            let current = source_rx.borrow_and_update().clone();
            if let Some(id) = current {
                break id;
            }
            if source_rx.changed().await.is_err() {
                return;
            }
        };

        let audio_routing = audio_routing_rx.borrow_and_update().clone();
        let audio_source_id = if audio_routing.audio_follows_video || audio_routing.active_audio_source.is_none() {
            current_source_id
        } else {
            audio_routing.active_audio_source.unwrap_or(current_source_id)
        };

        let same_source = current_source_id == audio_source_id;

        tracing::info!(
            "program router: video={} audio={} (same={})",
            current_source_id, audio_source_id, same_source
        );

        // Send sequence headers for initial source
        send_sequence_headers(&state, &current_source_id, &audio_source_id, output_ts).await;

        let video_relay = state.get_media_relay(&current_source_id).await;
        let audio_relay = if same_source {
            None
        } else {
            state.get_media_relay(&audio_source_id).await
        };

        let Some(video_tx) = video_relay else {
            tracing::warn!("program router: no relay for video source {}", current_source_id);
            if source_rx.changed().await.is_err() { return; }
            continue;
        };

        let mut video_rx = video_tx.subscribe();
        let mut audio_rx = audio_relay.map(|tx| tx.subscribe());

        let mut forwarded: u64 = 0;
        let mut got_keyframe = false;
        let mut first_video_ts: Option<u32> = None;
        let mut first_audio_ts: Option<u32> = None;
        let base_output_ts = output_ts;

        // Pending switch state: when a source change is requested, we subscribe
        // to the new source but keep forwarding the old one until we get a keyframe.
        let mut pending_switch: Option<PendingSwitch> = None;

        loop {
            tokio::select! {
                // Source changed
                result = source_rx.changed() => {
                    if result.is_err() { return; }
                    let new_id = *source_rx.borrow_and_update();
                    if let Some(new_source_id) = new_id {
                        if new_source_id == current_source_id {
                            continue;
                        }
                        // Subscribe to new source but keep forwarding old one
                        if let Some(new_relay) = state.get_media_relay(&new_source_id).await {
                            tracing::info!(
                                "program router: preparing switch {} -> {} (keeping old until keyframe)",
                                current_source_id, new_source_id
                            );
                            pending_switch = Some(PendingSwitch {
                                source_id: new_source_id,
                                rx: new_relay.subscribe(),
                                got_keyframe: false,
                            });
                        } else {
                            tracing::warn!("program router: no relay for new source {}, waiting", new_source_id);
                            pending_switch = Some(PendingSwitch {
                                source_id: new_source_id,
                                rx: broadcast::channel(1).0.subscribe(),
                                got_keyframe: false,
                            });
                        }
                    }
                }
                // Audio routing changed
                result = audio_routing_rx.changed() => {
                    if result.is_err() { return; }
                    // Re-enter outer loop to reconfigure audio
                    tracing::info!("program router: audio routing change");
                    break;
                }
                // New source data (when switch is pending)
                msg = async {
                    match &mut pending_switch {
                        Some(ps) => ps.rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match msg {
                        Ok(data) => {
                            if let Some(ref mut ps) = pending_switch {
                                let tt = tag_type(&data);
                                // Skip sequence headers, we'll send our own
                                if is_sequence_header(&data) {
                                    continue;
                                }
                                if tt == Some(FLV_TAG_VIDEO) && is_video_keyframe(&data) {
                                    ps.got_keyframe = true;
                                    // Send sequence headers for new source
                                    let new_audio_routing = audio_routing_rx.borrow().clone();
                                    let new_audio_id = if new_audio_routing.audio_follows_video || new_audio_routing.active_audio_source.is_none() {
                                        ps.source_id
                                    } else {
                                        new_audio_routing.active_audio_source.unwrap_or(ps.source_id)
                                    };
                                    send_sequence_headers(&state, &ps.source_id, &new_audio_id, output_ts).await;
                                    // Forward the keyframe
                                    let remapped = remap_tag(&data, &mut Some(flv::read_tag_timestamp(&data).unwrap_or(0)), output_ts, &mut output_ts);
                                    let _ = state.program_tx.send(remapped);
                                    tracing::info!(
                                        "program router: switched to {} after {} frames from old source",
                                        ps.source_id, forwarded
                                    );
                                    // Break to outer loop which will re-enter with new source
                                    pending_switch = None;
                                    break;
                                }
                                // Drop non-keyframe video from new source while waiting
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => {
                            // New source relay closed, abort switch
                            pending_switch = None;
                        }
                    }
                }
                // Current source data (always forward)
                msg = video_rx.recv() => {
                    match msg {
                        Ok(data) => {
                            let tt = tag_type(&data);

                            // Wait for first keyframe on initial connect
                            if !got_keyframe && tt == Some(FLV_TAG_VIDEO) {
                                if is_video_keyframe(&data) {
                                    got_keyframe = true;
                                } else if !is_sequence_header(&data) {
                                    continue;
                                }
                            }

                            if !got_keyframe && tt == Some(FLV_TAG_AUDIO) && !same_source {
                                continue;
                            }

                            if same_source || tt == Some(FLV_TAG_VIDEO) || is_sequence_header(&data) {
                                let remapped = remap_tag(&data, &mut first_video_ts, base_output_ts, &mut output_ts);
                                let _ = state.program_tx.send(remapped);
                                forwarded += 1;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                // Separate audio source data
                msg = async {
                    match &mut audio_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match msg {
                        Ok(data) => {
                            if tag_type(&data) == Some(FLV_TAG_AUDIO) {
                                let remapped = remap_tag(&data, &mut first_audio_ts, base_output_ts, &mut output_ts);
                                let _ = state.program_tx.send(remapped);
                                forwarded += 1;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => {
                            audio_rx = None;
                        }
                    }
                }
            }

            if forwarded == 1 || forwarded == 10 || forwarded % 1000 == 0 {
                tracing::debug!("program router: forwarded {} packets, output_ts={}", forwarded, output_ts);
            }
        }
    }
}

struct PendingSwitch {
    source_id: Uuid,
    rx: broadcast::Receiver<Bytes>,
    #[allow(dead_code)]
    got_keyframe: bool,
}

async fn send_sequence_headers(
    state: &AppState,
    video_source_id: &Uuid,
    audio_source_id: &Uuid,
    ts: u32,
) {
    let headers = state.sequence_headers.read().await;

    if let Some(seq) = headers.get(video_source_id) {
        if let Some(ref video) = seq.video {
            let remapped = flv::rewrite_tag_timestamp(video, ts);
            let _ = state.program_tx.send(remapped);
        }
    }

    let audio_seq = headers.get(audio_source_id).or_else(|| headers.get(video_source_id));
    if let Some(seq) = audio_seq {
        if let Some(ref audio) = seq.audio {
            let remapped = flv::rewrite_tag_timestamp(audio, ts);
            let _ = state.program_tx.send(remapped);
        }
    }
}
