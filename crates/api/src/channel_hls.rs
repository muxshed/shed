// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use bytes::Bytes;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};

use crate::routes::output::OutputConfig;
use crate::rtmp::flv;
use crate::state::SequenceHeaders;

/// Produces a public HLS rendition of the program output for the Channel watch page.
///
/// Uses ffmpeg — the media engine this codebase actually ships (the GStreamer
/// pipeline is feature-gated and never built). The program FLV stream is piped into
/// ffmpeg over stdin, exactly like [`crate::egress::EgressManager`], and segmented to
/// `{data_dir}/hls/{token}/index.m3u8` for the public watch page to play.
pub struct ChannelHls {
    process: Arc<Mutex<Option<Child>>>,
}

impl ChannelHls {
    pub fn new() -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn is_running(&self) -> bool {
        self.process.lock().await.is_some()
    }

    pub async fn stop(&self) {
        if let Some(mut child) = self.process.lock().await.take() {
            tracing::info!("stopping channel HLS");
            let _ = child.kill().await;
        }
    }

    pub async fn start(
        &self,
        token: &str,
        data_dir: &Path,
        media_tx: broadcast::Sender<Bytes>,
        output_config: Option<OutputConfig>,
        sequence_headers: Option<SequenceHeaders>,
    ) -> Result<(), String> {
        self.stop().await;

        let hls_dir = data_dir.join("hls").join(token);
        std::fs::create_dir_all(&hls_dir).map_err(|e| format!("cannot create hls dir: {}", e))?;
        // Clear stale playlist/segments from a previous run so viewers don't load old media.
        if let Ok(entries) = std::fs::read_dir(&hls_dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }

        let cfg = output_config.unwrap_or_default();
        let playlist = hls_dir.join("index.m3u8");
        let segment = hls_dir.join("seg%05d.ts");

        // Letterbox/pillarbox any input onto a fixed canvas so segments stay consistent
        // across source switches (same approach as egress).
        let vf = format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
            cfg.width, cfg.height, cfg.width, cfg.height
        );

        let args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "warning".into(),
            "-f".into(),
            "flv".into(),
            "-i".into(),
            "pipe:0".into(),
            "-vf".into(),
            vf,
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-tune".into(),
            "zerolatency".into(),
            "-b:v".into(),
            format!("{}k", cfg.video_bitrate_kbps),
            "-maxrate".into(),
            format!("{}k", cfg.video_bitrate_kbps),
            "-bufsize".into(),
            format!("{}k", cfg.video_bitrate_kbps * 2),
            "-g".into(),
            format!("{}", cfg.fps * 2),
            "-r".into(),
            format!("{}", cfg.fps),
            // Align keyframes to ~2s HLS segment boundaries.
            "-force_key_frames".into(),
            "expr:gte(t,n_forced*2)".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            format!("{}k", cfg.audio_bitrate_kbps),
            "-ar".into(),
            "48000".into(),
            "-f".into(),
            "hls".into(),
            "-hls_time".into(),
            "2".into(),
            "-hls_list_size".into(),
            "6".into(),
            "-hls_flags".into(),
            "delete_segments+independent_segments".into(),
            "-hls_segment_type".into(),
            "mpegts".into(),
            "-hls_segment_filename".into(),
            segment.display().to_string(),
            playlist.display().to_string(),
        ];

        tracing::info!("starting channel HLS → {}", playlist.display());

        let mut child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("failed to start ffmpeg for channel HLS: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to get channel HLS ffmpeg stdin".to_string())?;

        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::info!("ffmpeg [channel]: {}", line);
                }
            });
        }

        let mut rx = media_tx.subscribe();
        tokio::spawn(async move {
            let mut stdin = stdin;

            let header = flv::flv_header();
            if stdin.write_all(&header).await.is_err() {
                tracing::error!("channel HLS header write failed");
                return;
            }

            if let Some(ref seq) = sequence_headers {
                if let Some(ref video) = seq.video {
                    let _ = stdin.write_all(video).await;
                }
                if let Some(ref audio) = seq.audio {
                    let _ = stdin.write_all(audio).await;
                }
            }

            loop {
                match rx.recv().await {
                    Ok(data) => {
                        if stdin.write_all(&data).await.is_err() {
                            tracing::warn!("channel HLS pipe broken");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("channel HLS lagged {} packets", n);
                        continue;
                    }
                    Err(_) => {
                        tracing::info!("channel HLS program stream closed");
                        break;
                    }
                }
            }
        });

        *self.process.lock().await = Some(child);
        Ok(())
    }
}

impl Default for ChannelHls {
    fn default() -> Self {
        Self::new()
    }
}
