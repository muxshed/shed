// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};

use crate::routes::output::OutputConfig;
use crate::rtmp::flv::{self, is_sequence_header, is_video_keyframe, is_video_sequence_header};
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
            // Downmix to stereo — the native AAC encoder rejects 5.1/6-channel input
            // ("Unsupported channel layout"), and destinations want stereo anyway.
            "-ac".into(),
            "2".into(),
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

        if tracing::enabled!(tracing::Level::DEBUG) {
            tokio::spawn(watch_first_playlist(playlist.clone()));

            let seq = sequence_headers.as_ref();
            tracing::debug!(
                "channel HLS: priming with video seq header: {}, audio seq header: {}, cached keyframe: {}",
                seq.is_some_and(|s| s.video.is_some()),
                seq.is_some_and(|s| s.audio.is_some()),
                seq.is_some_and(|s| s.last_keyframe.is_some())
            );
        }

        let mut rx = media_tx.subscribe();
        tokio::spawn(async move {
            bootstrap_stdin(stdin, &mut rx, sequence_headers).await;
        });

        *self.process.lock().await = Some(child);
        Ok(())
    }
}

/// Debug-only startup timing: logs how long ffmpeg took to write the first playlist.
async fn watch_first_playlist(playlist: PathBuf) {
    let started = Instant::now();
    loop {
        if playlist.exists() {
            tracing::debug!(
                "channel HLS: first playlist created after {} ms",
                started.elapsed().as_millis()
            );
            return;
        }
        if started.elapsed().as_secs() >= 60 {
            tracing::debug!(
                "channel HLS: playlist still missing after {} ms",
                started.elapsed().as_millis()
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Write FLV header + codec config, then feed program tags starting at a keyframe.
async fn bootstrap_stdin(
    mut stdin: tokio::process::ChildStdin,
    rx: &mut broadcast::Receiver<Bytes>,
    sequence_headers: Option<SequenceHeaders>,
) {
    let started = Instant::now();

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
        // Priming keyframe (same approach as egress/preview) so ffmpeg can decode
        // immediately even if the next live packets are mid-GOP.
        if let Some(ref keyframe) = seq.last_keyframe {
            let _ = stdin.write_all(keyframe).await;
        }
    }

    tracing::debug!("channel HLS: waiting for first keyframe");
    let mut waiting_for_keyframe = true;

    loop {
        match rx.recv().await {
            Ok(data) => {
                if waiting_for_keyframe {
                    // Always accept mid-stream codec config (source switches).
                    if is_sequence_header(&data) {
                        if stdin.write_all(&data).await.is_err() {
                            tracing::warn!("channel HLS pipe broken");
                            break;
                        }
                        continue;
                    }
                    if is_video_keyframe(&data) {
                        waiting_for_keyframe = false;
                        tracing::debug!(
                            "channel HLS: received first keyframe after {} ms",
                            started.elapsed().as_millis()
                        );
                    } else {
                        // Drop audio/P-frames until we have a clean IDR after headers.
                        continue;
                    }
                } else if is_video_sequence_header(&data) {
                    // New AVC config mid-stream (source switch) — re-gate until the
                    // matching keyframe so ffmpeg does not stall on an open GOP.
                    waiting_for_keyframe = true;
                    tracing::debug!("channel HLS: new video sequence header; waiting for keyframe");
                    if stdin.write_all(&data).await.is_err() {
                        tracing::warn!("channel HLS pipe broken");
                        break;
                    }
                    continue;
                }

                if stdin.write_all(&data).await.is_err() {
                    tracing::warn!("channel HLS pipe broken");
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    "channel HLS lagged {} packets; waiting for next keyframe",
                    n
                );
                waiting_for_keyframe = true;
            }
            Err(_) => {
                tracing::info!("channel HLS program stream closed");
                break;
            }
        }
    }
}

impl Default for ChannelHls {
    fn default() -> Self {
        Self::new()
    }
}
