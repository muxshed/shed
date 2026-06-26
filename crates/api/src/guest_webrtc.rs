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
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
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
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::util::Marshal;

use crate::routes::output::OutputConfig;
use crate::state::AppState;

/// Fixed payload types we rewrite forwarded RTP to, so the FFmpeg SDP is static.
const VP8_PT: u8 = 96;
const OPUS_PT: u8 = 111;

/// Local UDP port allocator for the FFmpeg <- RTP bridge (video, audio per guest).
static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

fn alloc_ports() -> (u16, u16) {
    let n = PORT_COUNTER.fetch_add(1, Ordering::Relaxed) % 1000;
    let base = 41000 + n * 2;
    (base, base + 1)
}

/// Accept a guest's WHIP offer, wire up the RTP -> FFmpeg -> relay bridge, and
/// return the SDP answer. The source row must already exist (created by the
/// caller); on any error the caller should tear it down.
pub async fn start_guest_ingest(
    state: Arc<AppState>,
    source_id: Uuid,
    offer_sdp: String,
) -> Result<String, String> {
    let (video_port, audio_port) = alloc_ports();

    let pc = build_peer(&state).await?;

    // Forward the guest's RTP to the FFmpeg UDP ports, rewriting the payload
    // type to our fixed values so the SDP FFmpeg reads is deterministic.
    let forward = Arc::new(
        UdpSocket::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("udp bind: {}", e))?,
    );
    let fwd = forward.clone();
    pc.on_track(Box::new(move |track, _receiver, _transceiver| {
        let fwd = fwd.clone();
        Box::pin(async move {
            let is_video = track.kind() == RTPCodecType::Video;
            let (pt, port) = if is_video {
                (VP8_PT, video_port)
            } else {
                (OPUS_PT, audio_port)
            };
            let addr = format!("127.0.0.1:{}", port);
            while let Ok((mut packet, _)) = track.read_rtp().await {
                packet.header.payload_type = pt;
                if let Ok(raw) = packet.marshal() {
                    let _ = fwd.send_to(&raw, &addr).await;
                }
            }
        })
    }));

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

    state.guest_peers.write().await.insert(source_id, pc);
    Ok(sdp)
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
            ..Default::default()
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
