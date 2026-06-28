// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

//! Standalone WHIP test client. Publishes a VP8 + Opus stream to a Muxshed guest
//! WHIP endpoint so the guest ingest path can be exercised end-to-end without a
//! browser or camera.
//!
//! Generate media once:
//!   ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 30 -c:v libvpx -b:v 1M video.ivf
//!   ffmpeg -f lavfi -i sine=frequency=440 -t 30 -c:a libopus -page_duration 20000 audio.ogg
//!
//! Then, with the server running and a guest invite created:
//!   cargo run --example whip_client -- http://localhost:8080/api/v1/guest/<token>/whip video.ivf audio.ogg

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS, MIME_TYPE_VP8};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::ivf_reader::IVFReader;
use webrtc::media::io::ogg_reader::OggReader;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

type AnyError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: whip_client <whip_url> <video.ivf> <audio.ogg>");
        std::process::exit(1);
    }
    let whip_url = args[1].clone();
    let video_file = args[2].clone();
    let audio_file = args[3].clone();

    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let pc = Arc::new(api.new_peer_connection(config).await?);

    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "muxshed-whip-test".to_owned(),
    ));
    let want_audio = audio_file != "none";
    let audio_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "audio".to_owned(),
        "muxshed-whip-test".to_owned(),
    ));
    // NOTE: audio added FIRST (m-line 0), video SECOND (m-line 1) for the ordering test.
    if want_audio {
        pc.add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await?;
    }
    pc.add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let (connected_tx, mut connected_rx) = tokio::sync::mpsc::channel::<()>(1);
    pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        println!("[whip-client] pc state: {s:?}");
        let tx = connected_tx.clone();
        Box::pin(async move {
            if s == RTCPeerConnectionState::Connected {
                let _ = tx.try_send(());
            }
        })
    }));

    // Offer → gather ICE (non-trickle) → WHIP POST → answer.
    let offer = pc.create_offer(None).await?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(offer).await?;
    let _ = gather.recv().await;
    let local = pc.local_description().await.ok_or("no local description")?;

    println!("[whip-client] POST offer → {whip_url}");
    let answer_sdp = whip_post(&whip_url, &local.sdp).await?;
    println!("[whip-client] --- OFFER m-lines / ssrc ---");
    for l in local.sdp.lines().filter(|l| l.starts_with("m=") || l.starts_with("a=ssrc:") || l.starts_with("a=msid")) {
        println!("  offer: {l}");
    }
    println!("[whip-client] --- ANSWER m-lines ---");
    for l in answer_sdp.lines().filter(|l| {
        l.starts_with("m=") || l.starts_with("a=rtpmap") || l.contains("sendonly") || l.contains("recvonly") || l.contains("inactive")
    }) {
        println!("  answer: {l}");
    }
    pc.set_remote_description(RTCSessionDescription::answer(answer_sdp)?)
        .await?;

    tokio::select! {
        _ = connected_rx.recv() => println!("[whip-client] connected — streaming media"),
        _ = tokio::time::sleep(Duration::from_secs(15)) => {
            eprintln!("[whip-client] timed out waiting for connection (check ICE)");
        }
    }

    let vt = video_track.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = stream_ivf(&vt, &video_file).await {
                eprintln!("[whip-client] video error: {e}");
                break;
            }
        }
    });
    if want_audio {
        let at = audio_track.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = stream_ogg(&at, &audio_file).await {
                    eprintln!("[whip-client] audio error: {e}");
                    break;
                }
            }
        });
    }

    println!("[whip-client] streaming (Ctrl+C to stop)…");
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn sample(data: Bytes, duration: Duration) -> Sample {
    Sample {
        data,
        timestamp: SystemTime::now(),
        duration,
        packet_timestamp: 0,
        prev_dropped_packets: 0,
        prev_padding_packets: 0,
    }
}

async fn stream_ivf(track: &Arc<TrackLocalStaticSample>, path: &str) -> Result<(), AnyError> {
    let reader = BufReader::new(File::open(path)?);
    let (mut ivf, header) = IVFReader::new(reader)?;
    let fps = (header.timebase_denominator as f64 / header.timebase_numerator.max(1) as f64).max(1.0);
    let frame_dur = Duration::from_secs_f64(1.0 / fps);
    println!("[whip-client] video {}x{} @ {fps}fps", header.width, header.height);
    let mut ticker = tokio::time::interval(frame_dur);
    let mut n: u64 = 0;
    loop {
        let frame = match ivf.parse_next_frame() {
            Ok((f, _)) => f,
            Err(e) => {
                println!("[whip-client] video EOF after {n} frames ({e})");
                return Ok(()); // caller loops the file
            }
        };
        track.write_sample(&sample(frame.freeze(), frame_dur)).await?;
        if n == 0 {
            println!("[whip-client] sent first video sample");
        }
        n += 1;
        ticker.tick().await;
    }
}

async fn stream_ogg(track: &Arc<TrackLocalStaticSample>, path: &str) -> Result<(), AnyError> {
    let reader = BufReader::new(File::open(path)?);
    let (mut ogg, _hdr) = OggReader::new(reader, true)?;
    let mut ticker = tokio::time::interval(Duration::from_millis(20));
    let mut last_granule: u64 = 0;
    loop {
        let (page, hdr) = match ogg.parse_next_page() {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let samples = hdr.granule_position.saturating_sub(last_granule);
        last_granule = hdr.granule_position;
        let ms = (samples.saturating_mul(1000) / 48000).max(20);
        track
            .write_sample(&sample(page.freeze(), Duration::from_millis(ms)))
            .await?;
        ticker.tick().await;
    }
}

/// Minimal raw HTTP POST for the WHIP handshake (localhost, `application/sdp`).
async fn whip_post(url: &str, sdp: &str) -> Result<String, AnyError> {
    let rest = url.strip_prefix("http://").ok_or("only http:// supported")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host_port = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };

    let mut stream = TcpStream::connect(&host_port).await?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/sdp\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{sdp}",
        len = sdp.len()
    );
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("malformed HTTP response")?;
    let status = head.lines().next().unwrap_or("");
    if !status.contains(" 201") && !status.contains(" 200") {
        return Err(format!("WHIP server returned: {status} | {body}").into());
    }
    Ok(body.to_string())
}
