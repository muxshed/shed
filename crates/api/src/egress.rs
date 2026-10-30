// Licensed under the GNU Affero General Public License v3.0 Ã¢â‚¬â€ see LICENSE.

use bytes::Bytes;
use muxshed_common::{Destination, DestinationKind, WsEvent};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::routes::output::OutputStats;
use crate::rtmp::flv;

type RestartParams = (
    Vec<Destination>,
    broadcast::Sender<Bytes>,
    Option<crate::routes::output::OutputConfig>,
);

pub struct EgressManager {
    processes: Arc<Mutex<HashMap<Uuid, EgressProcess>>>,
    ws_tx: broadcast::Sender<WsEvent>,
    bytes_sent: Arc<std::sync::atomic::AtomicU64>,
    /// Monotonic time shared with the FFmpeg stdin writers.
    clock: Arc<Instant>,
    started_at: Arc<Mutex<Option<std::time::Instant>>>,
    /// The most recent start() params, so the egress can be cleanly restarted
    /// (e.g. on a program failover splice) with a fresh encoder.
    restart_params: Arc<Mutex<Option<RestartParams>>>,
}

struct EgressProcess {
    child: Child,
    destination_name: String,
    started_ms: u64,
    last_write_ms: Arc<AtomicU64>,
}

/// Local FFmpeg process health. This does not verify YouTube's ingest status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EgressHealth {
    pub destination_id: Uuid,
    pub destination_name: String,
    pub process_alive: bool,
    pub running_for_seconds: u64,
    pub last_media_write_age_seconds: Option<u64>,
    pub media_started: bool,
}


impl EgressManager {
    pub fn new(ws_tx: broadcast::Sender<WsEvent>) -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            ws_tx,
            bytes_sent: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            clock: Arc::new(Instant::now()),
            started_at: Arc::new(Mutex::new(None)),
            restart_params: Arc::new(Mutex::new(None)),
        }
    }

    /// Restart the egress with the params from the last start(), priming with the
    /// given source's sequence headers. Used when the program splices to a new
    /// source (failover) Ã¢â‚¬â€ a fresh encoder produces a clean stream and reconnects
    /// the destination, instead of the running encoder choking on the splice.
    pub async fn restart(&self, seq_headers: Option<crate::state::SequenceHeaders>) {
        let params = self.restart_params.lock().await.clone();
        let Some((destinations, media_tx, output_config)) = params else {
            return;
        };
        tracing::info!("restarting egress with a fresh encoder (program splice)");
        self.stop().await;
        // Let the old RTMP session fully close before reopening the same stream
        // key (some platforms reject a second publisher on an already-open key).
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let _ = self
            .start(Uuid::nil(), destinations, media_tx, output_config, seq_headers)
            .await;
    }

    /// Update the output config used on the next restart. Used to apply an
    /// output-config change (resolution, bitrate) to a live stream: set it, then
    /// call `restart` so the encoder comes back with the new settings.
    pub async fn set_output_config(&self, cfg: Option<crate::routes::output::OutputConfig>) {
        if let Some(params) = self.restart_params.lock().await.as_mut() {
            params.2 = cfg;
        }
    }

    pub async fn start(
        &self,
        _source_id: Uuid,
        destinations: Vec<Destination>,
        media_tx: broadcast::Sender<Bytes>,
        output_config: Option<crate::routes::output::OutputConfig>,
        sequence_headers: Option<crate::state::SequenceHeaders>,
    ) -> Result<(), String> {
        self.bytes_sent.store(0, std::sync::atomic::Ordering::Relaxed);
        *self.started_at.lock().await = Some(std::time::Instant::now());
        *self.restart_params.lock().await =
            Some((destinations.clone(), media_tx.clone(), output_config.clone()));
        let mut procs = self.processes.lock().await;

        for dest in &destinations {
            if !dest.enabled {
                continue;
            }

            let rtmp_url = match &dest.kind {
                DestinationKind::Rtmp { url, stream_key } => {
                    format!("{}/{}", url, stream_key)
                }
                DestinationKind::Rtmps { url, stream_key } => {
                    format!("{}/{}", url, stream_key)
                }
                _ => continue,
            };

            tracing::info!("starting egress to {} (destination URL withheld)", dest.name);

            let mut args = vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(), "info".to_string(),
                "-f".to_string(), "flv".to_string(),
                "-i".to_string(), "pipe:0".to_string(),
            ];

            // Always transcode with fixed output canvas.
            // Uses scale+pad to letterbox/pillarbox any input resolution onto the output canvas.
            // This prevents crashes when switching between sources with different resolutions.
            let cfg = output_config.as_ref().cloned().unwrap_or(crate::routes::output::OutputConfig::default());
            tracing::info!(
                "egress: compositing to {}x{}@{}fps {}kbps for {}",
                cfg.width, cfg.height, cfg.fps, cfg.video_bitrate_kbps, dest.name
            );

            // scale to fit within canvas preserving aspect ratio, then pad to exact canvas size (black bars)
            let vf = format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
                cfg.width, cfg.height, cfg.width, cfg.height
            );

            // Two independent overlays share the existing NVIDIA encoder.
            // The sponsor text remains visible. The weather textfile is blank
            // except for the configured 15-second pop-up window. A live edit
            // never restarts or reconnects YouTube; first enable requires the
            // next planned encoder start, as with the earlier lower-third.
            let vf = if std::path::Path::new("/tmp/muxshed-broadcast-overlay.enabled").exists() {
                format!(
                    concat!(
                        "{},",
                        "drawtext=fontfile=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf:",
                        "textfile=/tmp/muxshed-sponsor-overlay.txt:reload=1:expansion=none:",
                        "fontcolor=white:fontsize=h/23:x=w-text_w-32:y=h-text_h-30:",
                        "shadowcolor=black@0.80:shadowx=1:shadowy=2:",
                        "box=1:boxcolor=0x142236@0.94:boxborderw=16,",
                        "drawtext=fontfile=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf:",
                        "textfile=/tmp/muxshed-weather-popup.txt:reload=1:expansion=none:",
                        "fontcolor=white:fontsize=h/22:line_spacing=8:x=28:y=h-text_h-h*0.125:",
                        "shadowcolor=black@0.80:shadowx=1:shadowy=2:",
                        "box=1:boxcolor=0x0d2a42@0.94:boxborderw=20"
                    ),
                    vf
                )
            } else { vf };
            args.extend([
                "-vf".to_string(), vf,
                "-c:v".to_string(), "h264_nvenc".to_string(),
                "-preset".to_string(), "p4".to_string(),
                "-tune".to_string(), "ll".to_string(),
                "-b:v".to_string(), format!("{}k", cfg.video_bitrate_kbps),
                "-maxrate".to_string(), format!("{}k", cfg.video_bitrate_kbps),
                "-bufsize".to_string(), format!("{}k", cfg.video_bitrate_kbps * 2),
                "-g".to_string(), format!("{}", cfg.fps * 2),
                "-r".to_string(), format!("{}", cfg.fps),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), format!("{}k", cfg.audio_bitrate_kbps),
                "-ar".to_string(), "48000".to_string(),
                // Downmix to stereo: the native AAC encoder rejects 5.1/6-channel input.
                "-ac".to_string(), "2".to_string(),
            ]);

            args.extend([
                "-f".to_string(), "flv".to_string(),
                "-flvflags".to_string(), "no_duration_filesize".to_string(),
                rtmp_url.clone(),
            ]);

            let mut child = Command::new("ffmpeg")
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("failed to start ffmpeg for {}: {}", dest.name, e))?;

            let stdin = child.stdin.take()
                .ok_or_else(|| format!("failed to get stdin for {}", dest.name))?;

            let stderr = child.stderr.take();
            let log_name = dest.name.clone();
            if let Some(stderr) = stderr {
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        // FFmpeg errors often echo the complete RTMP URL and stream key.
                        if line.contains("rtmp://") || line.contains("rtmps://") {
                            tracing::info!("ffmpeg [{}]: [destination URL redacted]", log_name);
                        } else {
                            tracing::info!("ffmpeg [{}]: {}", log_name, line);
                        }
                    }
                });
            }

            let dest_id = dest.id;
            let dest_name = dest.name.clone();
            let ws_tx = self.ws_tx.clone();
            let mut rx = media_tx.subscribe();
            let bytes_counter = self.bytes_sent.clone();
            let seq_headers = sequence_headers.clone();
            let last_write_ms = Arc::new(AtomicU64::new(0));
            let writer_activity = last_write_ms.clone();
            let writer_clock = self.clock.clone();
            let started_ms = self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64;

            tokio::spawn(async move {
                let mut stdin = stdin;
                let mut bytes_written: u64 = 0;

                let header = flv::flv_header();
                if let Err(e) = stdin.write_all(&header).await {
                    tracing::error!("egress header write failed for {}: {}", dest_name, e);
                    return;
                }
                bytes_written += header.len() as u64;

                if let Some(ref seq) = seq_headers {
                    if let Some(ref video) = seq.video {
                        let _ = stdin.write_all(&flv::rewrite_tag_timestamp(video, 0)).await;
                        bytes_written += video.len() as u64;
                    }
                    if let Some(ref audio) = seq.audio {
                        let _ = stdin.write_all(&flv::rewrite_tag_timestamp(audio, 0)).await;
                        bytes_written += audio.len() as u64;
                    }
                }

                let mut first_live_ts: Option<u32> = None;
                let mut started_on_keyframe = false;
                loop {
                    match rx.recv().await {
                        Ok(data) => {
                            // Start a new output only on a live H.264 keyframe.
                            if !started_on_keyframe {
                                let keyframe = data.len() > 12
                                    && data[0] == 9
                                    && (data[11] & 0xF0) == 0x10
                                    && data[12] == 1;
                                if !keyframe {
                                    continue;
                                }
                                started_on_keyframe = true;
                            }
                            let len = data.len();
                            let rebased = if let Some(ts) = flv::read_tag_timestamp(&data) {
                                let is_media = data.len() > 12 && (data[0] == 8 || data[0] == 9);
                                let is_config = is_media && data[12] == 0;
                                let origin = if is_media && !is_config {
                                    *first_live_ts.get_or_insert(ts)
                                } else {
                                    first_live_ts.unwrap_or(ts)
                                };
                                flv::rewrite_tag_timestamp(&data, ts.saturating_sub(origin))
                            } else {
                                data.clone()
                            };
                            if let Err(e) = stdin.write_all(&rebased).await {
                                tracing::warn!("egress pipe broken for {} after {} bytes: {}", dest_name, bytes_written, e);
                                let _ = ws_tx.send(WsEvent::DestinationState {
                                    id: dest_id,
                                    state: "error".to_string(),
                                });
                                break;
                            }
                            // Count media only after FFmpeg accepted the bytes.
                            bytes_counter.fetch_add(len as u64, Ordering::Relaxed);
                            writer_activity.store(
                                writer_clock.elapsed().as_millis().min(u64::MAX as u128) as u64,
                                Ordering::Relaxed,
                            );
                            bytes_written += len as u64;
                            if bytes_written < 1000 || bytes_written % 1_000_000 < (len as u64) {
                                tracing::debug!("egress [{}]: {} bytes total", dest_name, bytes_written);
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("egress lagged {} for {}", n, dest_name);
                            continue;
                        }
                        Err(_) => {
                            tracing::info!("egress channel closed for {}", dest_name);
                            break;
                        }
                    }
                }
            });

            let _ = self.ws_tx.send(WsEvent::DestinationState {
                id: dest.id,
                state: "connected".to_string(),
            });

            procs.insert(dest.id, EgressProcess {
                child,
                destination_name: dest.name.clone(),
                started_ms,
                last_write_ms,
            });
        }

        Ok(())
    }

    pub async fn stop(&self) {
        let mut procs = self.processes.lock().await;
        for (id, mut proc) in procs.drain() {
            tracing::info!("stopping egress for {}", proc.destination_name);
            let _ = proc.child.kill().await;
            let _ = self.ws_tx.send(WsEvent::DestinationState {
                id,
                state: "disconnected".to_string(),
            });
        }
    }

    /// Inspect FFmpeg process liveness without touching its stream, source, or restart state.
    pub async fn health(&self) -> Vec<EgressHealth> {
        let now = self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let mut processes = self.processes.lock().await;
        processes.iter_mut().map(|(id, process)| {
            let process_alive = matches!(process.child.try_wait(), Ok(None));
            let last = process.last_write_ms.load(Ordering::Relaxed);
            EgressHealth {
                destination_id: *id,
                destination_name: process.destination_name.clone(),
                process_alive,
                running_for_seconds: now.saturating_sub(process.started_ms) / 1000,
                last_media_write_age_seconds: if last == 0 { None } else {
                    Some(now.saturating_sub(last) / 1000)
                },
                media_started: last != 0,
            }
        }).collect()
    }

    pub async fn is_running(&self) -> bool {
        !self.processes.lock().await.is_empty()
    }

    pub async fn stats(&self) -> OutputStats {
        let bytes = self.bytes_sent.load(std::sync::atomic::Ordering::Relaxed);
        let started = self.started_at.lock().await;
        let duration = started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        let bitrate = if duration > 0.0 {
            (bytes as f64 * 8.0) / (duration * 1000.0)
        } else {
            0.0
        };
        OutputStats {
            bytes_sent: bytes,
            duration_secs: duration,
            source_bitrate_kbps: bitrate,
            output_bitrate_kbps: 0,
            dropped_frames: 0,
            source_width: None,
            source_height: None,
            source_fps: None,
            source_encoder: None,
        }
    }
}
