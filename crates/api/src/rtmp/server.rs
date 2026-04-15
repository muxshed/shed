// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::rtmp::flv;
use crate::state::AppState;
use bytes::Bytes;
use muxshed_common::{SourceState, WsEvent};
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

pub async fn start_rtmp_server(state: Arc<AppState>, port: u16) {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind RTMP server on {}: {}", addr, e);
            return;
        }
    };

    tracing::info!("RTMP ingest listening on :{}", port);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!("RTMP connection from {}", addr);
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        tracing::warn!("RTMP connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                tracing::error!("RTMP accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut handshake = Handshake::new(PeerType::Server);
    let s0s1 = handshake.generate_outbound_p0_and_p1()?;
    stream.write_all(&s0s1).await?;

    let mut buf = [0u8; 8192];
    let mut handshake_complete = false;

    while !handshake_complete {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }

        match handshake.process_bytes(&buf[..n])? {
            HandshakeProcessResult::InProgress { response_bytes } => {
                stream.write_all(&response_bytes).await?;
            }
            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                stream.write_all(&response_bytes).await?;
                handshake_complete = true;

                if !remaining_bytes.is_empty() {
                    handle_session(stream, state, Some(remaining_bytes)).await?;
                    return Ok(());
                }
            }
        }
    }

    handle_session(stream, state, None).await
}

async fn handle_session(
    mut stream: TcpStream,
    state: Arc<AppState>,
    initial_bytes: Option<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerSessionConfig::new();
    let (mut session, initial_results) = ServerSession::new(config)?;

    for result in initial_results {
        if let ServerSessionResult::OutboundResponse(packet) = result {
            stream.write_all(&packet.bytes).await?;
        }
    }

    let mut active_stream_key: Option<String> = None;
    let mut source_id: Option<uuid::Uuid> = None;
    let mut media_tx: Option<broadcast::Sender<Bytes>> = None;

    if let Some(bytes) = initial_bytes {
        let results = session.handle_input(&bytes)?;
        process_results(
            &mut session,
            &mut stream,
            &state,
            results,
            &mut active_stream_key,
            &mut source_id,
            &mut media_tx,
        )
        .await?;
    }

    let mut buf = vec![0u8; 65536];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        let results = session.handle_input(&buf[..n])?;
        process_results(
            &mut session,
            &mut stream,
            &state,
            results,
            &mut active_stream_key,
            &mut source_id,
            &mut media_tx,
        )
        .await?;
    }

    if let Some(sid) = source_id {
        tracing::info!("RTMP source disconnected: {}", sid);
        crate::source_normalizer::stop_normalizer(&state, &sid).await;
        state.remove_media_relay(&sid).await;
        {
            let mut headers = state.sequence_headers.write().await;
            headers.remove(&sid);
        }
        {
            let mut states = state.source_states.write().await;
            states.insert(sid, SourceState::Disconnected);
        }
        let _ = state.ws_tx.send(WsEvent::SourceState {
            id: sid,
            state: SourceState::Disconnected,
        });
    }

    Ok(())
}

async fn process_results(
    session: &mut ServerSession,
    stream: &mut TcpStream,
    state: &Arc<AppState>,
    results: Vec<ServerSessionResult>,
    active_stream_key: &mut Option<String>,
    source_id: &mut Option<uuid::Uuid>,
    media_tx: &mut Option<broadcast::Sender<Bytes>>,
) -> Result<(), Box<dyn std::error::Error>> {
    for result in results {
        match result {
            ServerSessionResult::OutboundResponse(packet) => {
                stream.write_all(&packet.bytes).await?;
            }
            ServerSessionResult::RaisedEvent(event) => {
                handle_event(session, stream, state, event, active_stream_key, source_id, media_tx)
                    .await?;
            }
            ServerSessionResult::UnhandleableMessageReceived(_) => {}
        }
    }
    Ok(())
}

async fn handle_event(
    session: &mut ServerSession,
    stream: &mut TcpStream,
    state: &Arc<AppState>,
    event: ServerSessionEvent,
    active_stream_key: &mut Option<String>,
    source_id: &mut Option<uuid::Uuid>,
    media_tx: &mut Option<broadcast::Sender<Bytes>>,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        ServerSessionEvent::ConnectionRequested {
            request_id,
            app_name,
        } => {
            tracing::info!("RTMP connect request for app: {}", app_name);
            let results = session.accept_request(request_id)?;
            for result in results {
                if let ServerSessionResult::OutboundResponse(packet) = result {
                    stream.write_all(&packet.bytes).await?;
                }
            }
        }

        ServerSessionEvent::ReleaseStreamRequested {
            request_id,
            stream_key,
            ..
        } => {
            tracing::debug!("RTMP release stream: {}", stream_key);
            let results = session.accept_request(request_id)?;
            for result in results {
                if let ServerSessionResult::OutboundResponse(packet) = result {
                    stream.write_all(&packet.bytes).await?;
                }
            }
        }

        ServerSessionEvent::PublishStreamRequested {
            request_id,
            stream_key,
            ..
        } => {
            tracing::info!("RTMP publish request for key: {}", stream_key);

            let row = sqlx::query_as::<_, SourceRow>(
                "SELECT id, kind FROM sources WHERE kind LIKE ?",
            )
            .bind(format!("%\"stream_key\":\"{}\"%%", stream_key))
            .fetch_optional(&state.db)
            .await;

            match row {
                Ok(Some(row)) => {
                    let sid: uuid::Uuid = row.id.parse().unwrap_or_default();
                    tracing::info!("RTMP source matched: {} ({})", sid, stream_key);

                    *active_stream_key = Some(stream_key);
                    *source_id = Some(sid);

                    // Start source normalizer: raw RTMP -> FFmpeg -> normalized FLV relay
                    match crate::source_normalizer::start_normalizer(state.clone(), sid).await {
                        Ok(raw_tx) => {
                            *media_tx = Some(raw_tx);
                        }
                        Err(e) => {
                            tracing::error!("failed to start normalizer for {}: {}", sid, e);
                            // Fallback: write directly to relay (no normalization)
                            let tx = state.get_or_create_media_relay(sid).await;
                            *media_tx = Some(tx);
                        }
                    }

                    let results = session.accept_request(request_id)?;
                    for result in results {
                        if let ServerSessionResult::OutboundResponse(packet) = result {
                            stream.write_all(&packet.bytes).await?;
                        }
                    }

                    {
                        let mut states = state.source_states.write().await;
                        states.insert(sid, SourceState::Live);
                    }
                    let _ = state.ws_tx.send(WsEvent::SourceState {
                        id: sid,
                        state: SourceState::Live,
                    });
                }
                _ => {
                    tracing::warn!("RTMP publish rejected — unknown stream key: {}", stream_key);
                    let results = session.reject_request(
                        request_id,
                        "NetStream.Publish.Denied",
                        "Invalid stream key",
                    )?;
                    for result in results {
                        if let ServerSessionResult::OutboundResponse(packet) = result {
                            stream.write_all(&packet.bytes).await?;
                        }
                    }
                    stream.shutdown().await?;
                    return Err("invalid stream key".into());
                }
            }
        }

        ServerSessionEvent::PublishStreamFinished {
            stream_key, ..
        } => {
            tracing::info!("RTMP publish finished: {}", stream_key);
            if let Some(sid) = source_id.take() {
                crate::source_normalizer::stop_normalizer(state, &sid).await;
                state.remove_media_relay(&sid).await;
                {
                    let mut headers = state.sequence_headers.write().await;
                    headers.remove(&sid);
                }
                {
                    let mut states = state.source_states.write().await;
                    states.insert(sid, SourceState::Disconnected);
                }
                let _ = state.ws_tx.send(WsEvent::SourceState {
                    id: sid,
                    state: SourceState::Disconnected,
                });
            }
            *active_stream_key = None;
            *media_tx = None;
        }

        ServerSessionEvent::AudioDataReceived {
            data, timestamp, ..
        } => {
            if let Some(tx) = media_tx {
                let tag = flv::flv_audio_tag(&data, timestamp.value);
                let _ = tx.send(tag);
            }
        }

        ServerSessionEvent::VideoDataReceived {
            data, timestamp, ..
        } => {
            if let Some(tx) = media_tx {
                let tag = flv::flv_video_tag(&data, timestamp.value);
                let _ = tx.send(tag);
            }
        }

        ServerSessionEvent::StreamMetadataChanged { metadata, .. } => {
            tracing::info!("RTMP metadata: {}x{} {}fps",
                metadata.video_width.unwrap_or(0),
                metadata.video_height.unwrap_or(0),
                metadata.video_frame_rate.unwrap_or(0.0));
            if let Some(sid) = source_id {
                let info = crate::state::SourceMediaInfo {
                    width: metadata.video_width,
                    height: metadata.video_height,
                    fps: metadata.video_frame_rate.map(|f| f as f64),
                    video_bitrate_kbps: metadata.video_bitrate_kbps,
                    audio_bitrate_kbps: metadata.audio_bitrate_kbps,
                    audio_sample_rate: metadata.audio_sample_rate,
                    encoder: metadata.encoder.clone(),
                };
                let mut infos = state.source_media_info.write().await;
                infos.insert(*sid, info);
            }
        }

        _ => {}
    }

    Ok(())
}

#[derive(sqlx::FromRow)]
struct SourceRow {
    id: String,
    #[allow(dead_code)]
    kind: String,
}
