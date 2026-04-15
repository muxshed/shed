// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use bytes::Bytes;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::state::AppState;

/// Start playing a media file into the source's media relay.
/// For videos: loops based on loop_mode. For images: generates a continuous stream.
/// Returns a handle that can be used to stop playback.
pub async fn start_media_playback(
    state: Arc<AppState>,
    source_id: Uuid,
    file_path: &Path,
    loop_mode: &str,
) -> Result<(), String> {
    let relay_tx = state.get_or_create_media_relay(source_id).await;

    let is_image = matches!(
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
    );

    let should_loop = is_image || loop_mode == "loop";

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
    ];

    if should_loop && !is_image {
        args.extend(["-stream_loop".into(), "-1".into()]);
    }

    if is_image {
        args.extend([
            "-loop".into(),
            "1".into(),
            "-framerate".into(),
            "30".into(),
        ]);
    }

    args.extend([
        "-re".into(),
        "-i".into(),
        file_path.to_string_lossy().into_owned(),
    ]);

    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-tune".into(),
        "zerolatency".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-g".into(),
        "60".into(),
        "-r".into(),
        "30".into(),
        "-b:v".into(),
        "2000k".into(),
    ]);

    if is_image {
        args.extend(["-t".into(), "86400".into()]);
        args.extend(["-an".into()]);
    } else {
        args.extend([
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-ar".into(),
            "48000".into(),
        ]);
    }

    args.extend(["-f".into(), "flv".into(), "pipe:1".into()]);

    tracing::info!(
        "starting media playback for source {} from {:?} (loop={}, image={})",
        source_id,
        file_path,
        should_loop,
        is_image
    );

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to start ffmpeg: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("failed to capture ffmpeg stdout")?;

    if let Some(stderr) = child.stderr.take() {
        let sid = source_id;
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::debug!("ffmpeg media [{}]: {}", sid, line.trim()),
                    Err(_) => break,
                }
            }
        });
    }

    {
        let mut players = state.media_players.write().await;
        if let Some(mut old) = players.remove(&source_id) {
            let _ = old.kill().await;
        }
        players.insert(source_id, child);
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        feed_relay(source_id, stdout, relay_tx, state_clone).await;
    });

    Ok(())
}

async fn feed_relay(
    source_id: Uuid,
    mut stdout: tokio::process::ChildStdout,
    relay_tx: broadcast::Sender<Bytes>,
    state: Arc<AppState>,
) {
    let mut header = [0u8; 13];
    if stdout.read_exact(&mut header).await.is_err() {
        tracing::warn!("failed to read FLV header for media source {}", source_id);
        return;
    }

    let mut frame_count: u64 = 0;

    loop {
        let mut tag_header = [0u8; 11];
        match stdout.read_exact(&mut tag_header).await {
            Ok(_) => {}
            Err(_) => {
                tracing::info!("media playback ended for source {}", source_id);
                break;
            }
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

        let total_size = 11 + data_size as usize + 4;
        let mut tag_buf = Vec::with_capacity(total_size);
        tag_buf.extend_from_slice(&tag_header);
        tag_buf.extend_from_slice(&data);
        tag_buf.extend_from_slice(&prev_size);
        let tag = Bytes::from(tag_buf);

        if tag_type == 9 && !data.is_empty() {
            let frame_type = (data[0] >> 4) & 0x0F;
            let avc_packet_type = if data.len() > 1 { data[1] } else { 255 };

            if avc_packet_type == 0 {
                let mut headers = state.sequence_headers.write().await;
                let entry = headers
                    .entry(source_id)
                    .or_insert_with(crate::state::SequenceHeaders::default);
                entry.video = Some(tag.clone());
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
                let entry = headers
                    .entry(source_id)
                    .or_insert_with(crate::state::SequenceHeaders::default);
                entry.audio = Some(tag.clone());
            }
        }

        let _ = relay_tx.send(tag);

        frame_count += 1;
        if frame_count == 1 {
            tracing::info!("media playback streaming for source {}", source_id);
        }
    }

    {
        let mut players = state.media_players.write().await;
        players.remove(&source_id);
    }
}

pub async fn stop_media_playback(state: &AppState, source_id: &Uuid) {
    let mut players = state.media_players.write().await;
    if let Some(mut child) = players.remove(source_id) {
        let _ = child.kill().await;
        tracing::info!("stopped media playback for source {}", source_id);
    }
    state.remove_media_relay(source_id).await;
    state.sequence_headers.write().await.remove(source_id);
}
