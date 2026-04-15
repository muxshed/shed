// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use bytes::Bytes;
use muxshed_common::{Destination, DestinationKind, WsEvent};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::routes::output::OutputStats;
use crate::rtmp::flv;

pub struct EgressManager {
    processes: Arc<Mutex<HashMap<Uuid, EgressProcess>>>,
    ws_tx: broadcast::Sender<WsEvent>,
    bytes_sent: Arc<std::sync::atomic::AtomicU64>,
    started_at: Arc<Mutex<Option<std::time::Instant>>>,
}

struct EgressProcess {
    child: Child,
    destination_name: String,
}

impl EgressManager {
    pub fn new(ws_tx: broadcast::Sender<WsEvent>) -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            ws_tx,
            bytes_sent: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            started_at: Arc::new(Mutex::new(None)),
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

            tracing::info!("starting egress to {} → {}", dest.name, rtmp_url);

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

            args.extend([
                "-vf".to_string(), vf,
                "-c:v".to_string(), "libx264".to_string(),
                "-preset".to_string(), "medium".to_string(),
                "-tune".to_string(), "zerolatency".to_string(),
                "-b:v".to_string(), format!("{}k", cfg.video_bitrate_kbps),
                "-maxrate".to_string(), format!("{}k", cfg.video_bitrate_kbps),
                "-bufsize".to_string(), format!("{}k", cfg.video_bitrate_kbps * 2),
                "-g".to_string(), format!("{}", cfg.fps * 2),
                "-r".to_string(), format!("{}", cfg.fps),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-c:a".to_string(), "aac".to_string(),
                "-b:a".to_string(), format!("{}k", cfg.audio_bitrate_kbps),
                "-ar".to_string(), "48000".to_string(),
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

            // Log ffmpeg stderr
            let stderr = child.stderr.take();
            let log_name = dest.name.clone();
            if let Some(stderr) = stderr {
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::info!("ffmpeg [{}]: {}", log_name, line);
                    }
                });
            }

            let dest_id = dest.id;
            let dest_name = dest.name.clone();
            let ws_tx = self.ws_tx.clone();
            let mut rx = media_tx.subscribe();
            let bytes_counter = self.bytes_sent.clone();
            let seq_headers = sequence_headers.clone();

            tokio::spawn(async move {
                let mut stdin = stdin;
                let mut bytes_written: u64 = 0;

                // Send FLV header
                let header = flv::flv_header();
                if let Err(e) = stdin.write_all(&header).await {
                    tracing::error!("egress header write failed for {}: {}", dest_name, e);
                    return;
                }
                bytes_written += header.len() as u64;

                // Send cached sequence headers so FFmpeg can decode immediately
                if let Some(ref seq) = seq_headers {
                    if let Some(ref video) = seq.video {
                        let _ = stdin.write_all(video).await;
                        bytes_written += video.len() as u64;
                    }
                    if let Some(ref audio) = seq.audio {
                        let _ = stdin.write_all(audio).await;
                        bytes_written += audio.len() as u64;
                    }
                }

                loop {
                    match rx.recv().await {
                        Ok(data) => {
                            let len = data.len();
                            bytes_counter.fetch_add(len as u64, std::sync::atomic::Ordering::Relaxed);
                            if let Err(e) = stdin.write_all(&data).await {
                                tracing::warn!("egress pipe broken for {} after {} bytes: {}", dest_name, bytes_written, e);
                                let _ = ws_tx.send(WsEvent::DestinationState {
                                    id: dest_id,
                                    state: "error".to_string(),
                                });
                                break;
                            }
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
