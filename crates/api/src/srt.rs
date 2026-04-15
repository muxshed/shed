// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SRT ingest listener. Each SRT source gets its own FFmpeg process that
//! listens on a dedicated UDP port, receives SRT input, normalizes to the
//! output canvas resolution, and feeds FLV into the media relay.

use bytes::Bytes;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::routes::output::OutputConfig;
use crate::state::AppState;

pub async fn start_srt_listener(
    state: Arc<AppState>,
    source_id: Uuid,
    port: u16,
    passphrase: Option<&str>,
) -> Result<(), String> {
    let cfg = load_output_config(&state).await;
    let public_tx = state.get_or_create_media_relay(source_id).await;

    let vf = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        cfg.width, cfg.height, cfg.width, cfg.height
    );
    let bv = format!("{}k", cfg.video_bitrate_kbps);
    let maxrate = format!("{}k", cfg.video_bitrate_kbps);
    let bufsize = format!("{}k", cfg.video_bitrate_kbps * 2);
    let gop = format!("{}", cfg.fps * 2);
    let fps = format!("{}", cfg.fps);
    let ba = format!("{}k", cfg.audio_bitrate_kbps);

    let mut srt_url = format!("srt://0.0.0.0:{}?mode=listener", port);
    if let Some(pw) = passphrase {
        if !pw.is_empty() {
            srt_url.push_str(&format!("&passphrase={}", pw));
        }
    }

    let args = vec![
        "-hide_banner",
        "-loglevel", "warning",
        "-i", &srt_url,
        "-vf", &vf,
        "-c:v", "libx264",
        "-preset", "veryfast",
        "-tune", "zerolatency",
        "-b:v", &bv,
        "-maxrate", &maxrate,
        "-bufsize", &bufsize,
        "-g", &gop,
        "-r", &fps,
        "-pix_fmt", "yuv420p",
        "-c:a", "aac",
        "-b:a", &ba,
        "-ar", "48000",
        "-f", "flv",
        "-flvflags", "no_duration_filesize",
        "pipe:1",
    ];

    tracing::info!(
        "starting SRT listener for source {} on port {} ({}x{}@{}fps)",
        source_id, port, cfg.width, cfg.height, cfg.fps
    );

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to start SRT listener ffmpeg: {}", e))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;

    if let Some(stderr) = child.stderr.take() {
        let sid = source_id;
        let st = state.clone();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            let mut marked_live = false;
            loop {
                line.clear();
                match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        tracing::debug!("srt [{}]: {}", sid, trimmed);
                        // Detect when FFmpeg starts receiving data
                        if !marked_live && (trimmed.contains("Output #0") || trimmed.contains("frame=")) {
                            marked_live = true;
                            st.source_states.write().await.insert(sid, muxshed_common::SourceState::Live);
                            let _ = st.ws_tx.send(muxshed_common::WsEvent::SourceState {
                                id: sid,
                                state: muxshed_common::SourceState::Live,
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
            // FFmpeg exited -- source disconnected
            st.source_states.write().await.insert(sid, muxshed_common::SourceState::Disconnected);
            let _ = st.ws_tx.send(muxshed_common::WsEvent::SourceState {
                id: sid,
                state: muxshed_common::SourceState::Disconnected,
            });
        });
    }

    {
        let mut listeners = state.srt_listeners.write().await;
        if let Some(mut old) = listeners.remove(&source_id) {
            let _ = old.kill().await;
        }
        listeners.insert(source_id, child);
    }

    // Mark as connecting (waiting for encoder)
    state.source_states.write().await.insert(source_id, muxshed_common::SourceState::Connecting);
    let _ = state.ws_tx.send(muxshed_common::WsEvent::SourceState {
        id: source_id,
        state: muxshed_common::SourceState::Connecting,
    });

    // Read normalized FLV from stdout and feed into relay
    let state_clone = state.clone();
    tokio::spawn(async move {
        read_flv_output(source_id, stdout, public_tx, state_clone).await;
    });

    Ok(())
}

async fn read_flv_output(
    source_id: Uuid,
    mut stdout: tokio::process::ChildStdout,
    public_tx: tokio::sync::broadcast::Sender<Bytes>,
    state: Arc<AppState>,
) {
    // Read and discard FLV header (13 bytes)
    let mut header = [0u8; 13];
    if stdout.read_exact(&mut header).await.is_err() {
        tracing::warn!("srt: failed to read FLV header for {}", source_id);
        return;
    }

    let mut frame_count: u64 = 0;

    loop {
        let mut tag_header = [0u8; 11];
        if stdout.read_exact(&mut tag_header).await.is_err() {
            break;
        }

        let tag_type = tag_header[0];
        let data_size = ((tag_header[1] as u32) << 16)
            | ((tag_header[2] as u32) << 8)
            | (tag_header[3] as u32);

        let mut data = vec![0u8; data_size as usize];
        if stdout.read_exact(&mut data).await.is_err() {
            break;
        }

        let mut prev_size = [0u8; 4];
        if stdout.read_exact(&mut prev_size).await.is_err() {
            break;
        }

        let total = 11 + data_size as usize + 4;
        let mut tag_buf = Vec::with_capacity(total);
        tag_buf.extend_from_slice(&tag_header);
        tag_buf.extend_from_slice(&data);
        tag_buf.extend_from_slice(&prev_size);
        let tag = Bytes::from(tag_buf);

        // Cache sequence headers
        if tag_type == 9 && !data.is_empty() {
            let avc_packet_type = if data.len() > 1 { data[1] } else { 255 };
            let frame_type = (data[0] >> 4) & 0x0F;
            if avc_packet_type == 0 {
                let mut headers = state.sequence_headers.write().await;
                headers.entry(source_id).or_default().video = Some(tag.clone());
            } else if frame_type == 1 {
                let mut headers = state.sequence_headers.write().await;
                if let Some(entry) = headers.get_mut(&source_id) {
                    entry.last_keyframe = Some(tag.clone());
                }
            }
        } else if tag_type == 8 && !data.is_empty() {
            let aac_packet_type = if data.len() > 1 { data[1] } else { 255 };
            if aac_packet_type == 0 {
                let mut headers = state.sequence_headers.write().await;
                headers.entry(source_id).or_default().audio = Some(tag.clone());
            }
        }

        // Mark live on first frame
        if frame_count == 0 {
            state.source_states.write().await.insert(source_id, muxshed_common::SourceState::Live);
            let _ = state.ws_tx.send(muxshed_common::WsEvent::SourceState {
                id: source_id,
                state: muxshed_common::SourceState::Live,
            });
            tracing::info!("srt: source {} is live", source_id);
        }

        let _ = public_tx.send(tag);
        frame_count += 1;
    }

    tracing::info!("srt: ended for {} after {} frames", source_id, frame_count);
}

pub async fn stop_srt_listener(state: &AppState, source_id: &Uuid) {
    let mut listeners = state.srt_listeners.write().await;
    if let Some(mut child) = listeners.remove(source_id) {
        let _ = child.kill().await;
        tracing::info!("stopped SRT listener for {}", source_id);
    }
    state.remove_media_relay(source_id).await;
    state.sequence_headers.write().await.remove(source_id);
}

/// Find the next available port in the SRT range for a new source.
pub async fn assign_srt_port(state: &AppState) -> u16 {
    let config = state.config.read().await;
    let start = config.srt_port_range_start;
    drop(config);

    let listeners = state.srt_listeners.read().await;
    let used_ports: std::collections::HashSet<u16> = listeners.keys().map(|_| 0u16).collect();
    drop(listeners);

    // Check which ports are actually in use by querying existing SRT sources
    let rows: Vec<(String,)> = sqlx::query_as("SELECT kind FROM sources WHERE kind LIKE '%\"srt\"%'")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let mut used: std::collections::HashSet<u16> = used_ports;
    for (kind_json,) in &rows {
        if let Ok(kind) = serde_json::from_str::<muxshed_common::SourceKind>(kind_json) {
            if let muxshed_common::SourceKind::Srt { port, .. } = kind {
                used.insert(port);
            }
        }
    }

    for p in start..start + 100 {
        if !used.contains(&p) {
            return p;
        }
    }
    start + 100
}

async fn load_output_config(state: &AppState) -> OutputConfig {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'output_config'")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|(json,)| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}
