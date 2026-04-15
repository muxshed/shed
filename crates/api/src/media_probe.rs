// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProbeResult {
    pub duration_ms: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
    pub fps: Option<f64>,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub bitrate_kbps: Option<u32>,
}

pub async fn probe_file(path: &Path) -> Option<ProbeResult> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        tracing::warn!("ffprobe failed for {}", path.display());
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;

    let mut result = ProbeResult::default();

    // Parse format
    if let Some(format) = json.get("format") {
        if let Some(dur) = format.get("duration").and_then(|v| v.as_str()) {
            if let Ok(secs) = dur.parse::<f64>() {
                result.duration_ms = (secs * 1000.0) as u64;
            }
        }
        if let Some(br) = format.get("bit_rate").and_then(|v| v.as_str()) {
            if let Ok(bps) = br.parse::<u64>() {
                result.bitrate_kbps = Some((bps / 1000) as u32);
            }
        }
    }

    // Parse streams
    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");

            if codec_type == "video" && result.width.is_none() {
                result.width = stream.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
                result.height = stream.get("height").and_then(|v| v.as_u64()).map(|v| v as u32);
                result.codec = stream.get("codec_name").and_then(|v| v.as_str()).map(String::from);

                // Parse FPS from r_frame_rate (e.g. "30/1" or "30000/1001")
                if let Some(rate) = stream.get("r_frame_rate").and_then(|v| v.as_str()) {
                    let parts: Vec<&str> = rate.split('/').collect();
                    if parts.len() == 2 {
                        if let (Ok(num), Ok(den)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                            if den > 0.0 {
                                result.fps = Some(num / den);
                            }
                        }
                    }
                }
            }

            if codec_type == "audio" && result.audio_codec.is_none() {
                result.audio_codec = stream.get("codec_name").and_then(|v| v.as_str()).map(String::from);
                result.audio_sample_rate = stream
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse().ok());
            }
        }
    }

    Some(result)
}

/// Generate a thumbnail from a video file. Seeks to 10% of duration or 1s,
/// skips black frames, and saves a JPEG.
pub async fn generate_thumbnail(video_path: &Path, output_dir: &Path, asset_id: &str) -> Option<PathBuf> {
    let thumb_path = output_dir.join(format!("{}_thumb.jpg", asset_id));

    // First try: seek to 1 second and grab a frame, skip black
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel", "error",
            "-ss", "1",
            "-i",
        ])
        .arg(video_path)
        .args([
            "-vf", "thumbnail=300,scale=320:-1",
            "-frames:v", "1",
            "-q:v", "5",
        ])
        .arg(&thumb_path)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() && thumb_path.exists() => {
            // Check if the thumbnail has reasonable size (not a black frame)
            if let Ok(meta) = tokio::fs::metadata(&thumb_path).await {
                if meta.len() > 500 {
                    tracing::debug!("thumbnail generated: {}", thumb_path.display());
                    return Some(thumb_path);
                }
            }

            // Fallback: try at 0.1s
            let _ = Command::new("ffmpeg")
                .args(["-y", "-hide_banner", "-loglevel", "error", "-ss", "0.1", "-i"])
                .arg(video_path)
                .args(["-vf", "scale=320:-1", "-frames:v", "1", "-q:v", "5"])
                .arg(&thumb_path)
                .output()
                .await;

            if thumb_path.exists() {
                return Some(thumb_path);
            }
            None
        }
        _ => {
            tracing::warn!("thumbnail generation failed for {}", video_path.display());
            None
        }
    }
}
