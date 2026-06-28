// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! WebRTC (WHIP) guest ingest.
//!
//! A guest publishes camera/mic over WebRTC. We terminate the peer with
//! webrtc-rs, forward the guest's RTP to local UDP ports, and run an FFmpeg
//! subprocess that reads them via an SDP, normalizes to the output canvas, and
//! feeds FLV into the source's media relay — so a guest becomes an ordinary,
//! switchable Source alongside RTMP/SRT/media-file sources.
//!
//! The live media path can only be verified against a real browser (and, off-LAN,
//! a TURN server); see `docs/guests-webrtc.md`.

use bytes::Bytes;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tokio::process::Command;
use tokio::sync::broadcast;
use uuid::Uuid;

use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::track::track_remote::TrackRemote;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::util::Marshal;

use crate::routes::output::OutputConfig;
use crate::state::AppState;

/// Fixed payload types we rewrite forwarded RTP to, so the FFmpeg SDP is static.
const VP8_PT: u8 = 96;
const OPUS_PT: u8 = 111;

/// Find a free UDP port pair for the FFmpeg <- RTP bridge by binding probe
/// sockets, so we never collide with a still-running ingest's ports. The probe
/// sockets are dropped before returning, freeing the ports for FFmpeg to bind.
async fn alloc_ports() -> Result<(u16, u16), String> {
    let s1 = UdpSocket::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("port probe: {}", e))?;
    let s2 = UdpSocket::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("port probe: {}", e))?;
    let p1 = s1.local_addr().map_err(|e| e.to_string())?.port();
    let p2 = s2.local_addr().map_err(|e| e.to_string())?.port();
    Ok((p1, p2))
}

/// Accept a guest's WHIP offer, wire up the RTP -> FFmpeg -> relay bridge, and
/// return the SDP answer. The source row must already exist (created by the
/// caller); on any error the caller should tear it down.
pub async fn start_guest_ingest(
    state: Arc<AppState>,
    source_id: Uuid,
    offer_sdp: String,
) -> Result<String, String> {
    let (video_port, audio_port) = alloc_ports().await?;

    let pc = build_peer(&state).await?;

    // Forward the guest's RTP to the FFmpeg UDP ports, rewriting the payload
    // type to our fixed values so the SDP FFmpeg reads is deterministic.
    let forward = Arc::new(
        UdpSocket::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("udp bind: {}", e))?,
    );
    // NOTE: we do NOT use pc.on_track — webrtc-rs only fires it for one of two
    // bundled tracks (audio), silently dropping the video track even though its
    // receiver is set up correctly. Instead we read tracks directly off the
    // transceivers' receivers (see the pump task below).

    // Tear the guest source down when the peer drops.
    let st = state.clone();
    pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        let st = st.clone();
        Box::pin(async move {
            tracing::info!("guest {} peer state: {:?}", source_id, s);
            if matches!(
                s,
                RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Closed
            ) {
                stop_guest_ingest(&st, &source_id).await;
            }
        })
    }));

    // Validate + apply the offer before spending any FFmpeg/transcode resources.
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| format!("bad offer: {}", e))?;
    pc.set_remote_description(offer)
        .await
        .map_err(|e| format!("set remote description: {}", e))?;

    // Offer is good — bring up the FFmpeg bridge so the ports are bound before
    // media starts flowing once the guest connects.
    let public_tx = state.get_or_create_media_relay(source_id).await;
    start_ffmpeg(state.clone(), source_id, video_port, audio_port, public_tx).await?;

    let answer = pc
        .create_answer(None)
        .await
        .map_err(|e| format!("create answer: {}", e))?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(answer)
        .await
        .map_err(|e| format!("set local description: {}", e))?;
    let _ = gather.recv().await;

    let sdp = pc
        .local_description()
        .await
        .ok_or("no local description after gathering")?
        .sdp;

    // Pump: poll the transceivers' receivers for tracks and forward each one's
    // RTP. This replaces on_track, which drops the second bundled track.
    let pump_pc = Arc::downgrade(&pc);
    let pump_fwd = forward.clone();
    tokio::spawn(async move {
        let mut started: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for _ in 0..60 {
            let Some(pc) = pump_pc.upgrade() else { break };
            for t in pc.get_transceivers().await {
                let is_video = t.kind() == RTPCodecType::Video;
                for track in t.receiver().await.tracks().await {
                    let ssrc = track.ssrc();
                    if ssrc == 0 || started.contains(&ssrc) {
                        continue;
                    }
                    started.insert(ssrc);
                    forward_track(
                        track,
                        is_video,
                        video_port,
                        audio_port,
                        pump_fwd.clone(),
                        source_id,
                        Arc::downgrade(&pc),
                    );
                }
            }
            drop(pc);
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    state.guest_peers.write().await.insert(source_id, pc);

    // Reap the guest if it never goes live (abandoned join, or ICE never
    // completes) so it doesn't leak an ffmpeg process, UDP ports, and a source.
    let watchdog = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let live = matches!(
            watchdog.source_states.read().await.get(&source_id),
            Some(muxshed_common::SourceState::Live)
        );
        if !live {
            tracing::warn!("guest {} did not go live within 30s; tearing down", source_id);
            stop_guest_ingest(&watchdog, &source_id).await;
        }
    });

    Ok(sdp)
}

/// Forward one remote track's RTP to its FFmpeg UDP port, rewriting the payload
/// type to our fixed value. For video, also drive periodic keyframe requests.
fn forward_track(
    track: Arc<TrackRemote>,
    is_video: bool,
    video_port: u16,
    audio_port: u16,
    fwd: Arc<UdpSocket>,
    source_id: Uuid,
    pli_pc: std::sync::Weak<RTCPeerConnection>,
) {
    tokio::spawn(async move {
        let kind = if is_video { "video" } else { "audio" };
        let (pt, port) = if is_video {
            (VP8_PT, video_port)
        } else {
            (OPUS_PT, audio_port)
        };
        tracing::info!("guest {}: forwarding {} RTP -> 127.0.0.1:{}", source_id, kind, port);

        // VP8 only decodes from a keyframe — periodically request one (PLI).
        if is_video {
            let ssrc = track.ssrc();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let Some(pc) = pli_pc.upgrade() else { break };
                    if pc
                        .write_rtcp(&[Box::new(PictureLossIndication {
                            sender_ssrc: 0,
                            media_ssrc: ssrc,
                        })])
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }

        let addr = format!("127.0.0.1:{}", port);
        let mut n: u64 = 0;
        while let Ok((mut packet, _)) = track.read_rtp().await {
            if n == 0 {
                tracing::info!(
                    "guest {}: first {} RTP packet (pt {}) -> :{}",
                    source_id, kind, packet.header.payload_type, port
                );
            }
            packet.header.payload_type = pt;
            if let Ok(raw) = packet.marshal() {
                if fwd.send_to(&raw, &addr).await.is_err() {
                    break;
                }
            }
            n += 1;
        }
        tracing::info!("guest {}: {} track ended after {} packets", source_id, kind, n);
    });
}

async fn build_peer(state: &AppState) -> Result<Arc<RTCPeerConnection>, String> {
    let mut m = MediaEngine::default();
    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            payload_type: VP8_PT,
            ..Default::default()
        },
        RTPCodecType::Video,
    )
    .map_err(|e| format!("register vp8: {}", e))?;
    m.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: OPUS_PT,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )
    .map_err(|e| format!("register opus: {}", e))?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)
        .map_err(|e| format!("interceptors: {}", e))?;

    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    let ice_servers = crate::routes::webrtc_config::load(state)
        .await
        .ice_servers
        .into_iter()
        .map(|s| RTCIceServer {
            urls: s.urls,
            username: s.username.unwrap_or_default(),
            credential: s.credential.unwrap_or_default(),
        })
        .collect();
    let config = RTCConfiguration {
        ice_servers,
        ..Default::default()
    };

    let pc = api
        .new_peer_connection(config)
        .await
        .map_err(|e| format!("new peer connection: {}", e))?;

    Ok(Arc::new(pc))
}

async fn start_ffmpeg(
    state: Arc<AppState>,
    source_id: Uuid,
    video_port: u16,
    audio_port: u16,
    public_tx: broadcast::Sender<Bytes>,
) -> Result<(), String> {
    let cfg = load_output_config(&state).await;

    let sdp = format!(
        "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=muxshed-guest\r\nc=IN IP4 127.0.0.1\r\nt=0 0\r\n\
         m=video {vp} RTP/AVP {vpt}\r\na=rtpmap:{vpt} VP8/90000\r\n\
         m=audio {ap} RTP/AVP {opt}\r\na=rtpmap:{opt} opus/48000/2\r\n",
        vp = video_port,
        vpt = VP8_PT,
        ap = audio_port,
        opt = OPUS_PT,
    );
    let data_dir = state.config.read().await.data_dir.clone();
    tokio::fs::create_dir_all(&data_dir).await.ok();
    let sdp_path = data_dir.join(format!("guest-{}.sdp", source_id));
    tokio::fs::write(&sdp_path, sdp)
        .await
        .map_err(|e| format!("write sdp: {}", e))?;
    let sdp_arg = sdp_path.to_string_lossy().to_string();

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

    let args = vec![
        "-hide_banner",
        "-loglevel", "warning",
        "-protocol_whitelist", "file,crypto,data,rtp,udp",
        "-fflags", "+genpts",
        // VP8 over RTP only reveals its frame size once a keyframe is decoded;
        // give ffmpeg time to find one instead of dropping the video stream.
        "-analyzeduration", "10000000",
        "-probesize", "10000000",
        "-i", &sdp_arg,
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
        "starting guest ingest ffmpeg for {} (video :{}, audio :{})",
        source_id, video_port, audio_port
    );

    let mut child = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to start guest ffmpeg: {}", e))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;

    if let Some(stderr) = child.stderr.take() {
        let sid = source_id;
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => tracing::debug!("guest [{}]: {}", sid, line.trim()),
                    Err(_) => break,
                }
            }
        });
    }

    {
        let mut normalizers = state.source_normalizers.write().await;
        if let Some(mut old) = normalizers.remove(&source_id) {
            let _ = old.kill().await;
        }
        normalizers.insert(source_id, child);
    }

    state
        .source_states
        .write()
        .await
        .insert(source_id, muxshed_common::SourceState::Connecting);
    let _ = state.ws_tx.send(muxshed_common::WsEvent::SourceState {
        id: source_id,
        state: muxshed_common::SourceState::Connecting,
    });

    tokio::spawn(async move {
        read_flv_output(source_id, stdout, public_tx, state).await;
    });

    Ok(())
}

async fn read_flv_output(
    source_id: Uuid,
    mut stdout: tokio::process::ChildStdout,
    public_tx: broadcast::Sender<Bytes>,
    state: Arc<AppState>,
) {
    let mut header = [0u8; 13];
    if stdout.read_exact(&mut header).await.is_err() {
        tracing::warn!("guest: failed to read FLV header for {}", source_id);
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

        if frame_count == 0 {
            state
                .source_states
                .write()
                .await
                .insert(source_id, muxshed_common::SourceState::Live);
            let _ = state.ws_tx.send(muxshed_common::WsEvent::SourceState {
                id: source_id,
                state: muxshed_common::SourceState::Live,
            });
            // Mark the guest record connected now that media is flowing.
            let _ = sqlx::query("UPDATE guests SET status = 'connected' WHERE source_id = ?")
                .bind(source_id.to_string())
                .execute(&state.db)
                .await;
            tracing::info!("guest: source {} is live", source_id);
        }

        let _ = public_tx.send(tag);
        frame_count += 1;
    }

    tracing::info!("guest: ended for {} after {} frames", source_id, frame_count);
}

/// Tear down a guest's ingest: stop FFmpeg, close the peer, drop the ephemeral
/// source row, and reset the guest record.
pub async fn stop_guest_ingest(state: &AppState, source_id: &Uuid) {
    {
        let mut normalizers = state.source_normalizers.write().await;
        if let Some(mut child) = normalizers.remove(source_id) {
            let _ = child.kill().await;
        }
    }
    if let Some(pc) = state.guest_peers.write().await.remove(source_id) {
        let _ = pc.close().await;
    }
    state.remove_media_relay(source_id).await;
    state.sequence_headers.write().await.remove(source_id);
    state
        .source_states
        .write()
        .await
        .insert(*source_id, muxshed_common::SourceState::Disconnected);
    let _ = state.ws_tx.send(muxshed_common::WsEvent::SourceState {
        id: *source_id,
        state: muxshed_common::SourceState::Disconnected,
    });

    let sid = source_id.to_string();
    let _ = sqlx::query("DELETE FROM sources WHERE id = ?")
        .bind(&sid)
        .execute(&state.db)
        .await;
    let _ = sqlx::query("UPDATE guests SET status = 'left', source_id = NULL WHERE source_id = ?")
        .bind(&sid)
        .execute(&state.db)
        .await;

    let data_dir = state.config.read().await.data_dir.clone();
    let _ = tokio::fs::remove_file(data_dir.join(format!("guest-{}.sdp", source_id))).await;
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
