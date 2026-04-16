// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::Response;
use std::sync::Arc;

use crate::rtmp::flv;
use crate::state::AppState;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    ws.on_upgrade(move |socket| handle_preview(socket, state, id))
}

async fn handle_preview(mut socket: WebSocket, state: Arc<AppState>, id: String) {
    let source_id: uuid::Uuid = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let relay = state.get_media_relay(&source_id).await;
    let Some(tx) = relay else {
        tracing::debug!("no active media relay for source {}", source_id);
        let _ = socket.send(Message::Close(None)).await;
        return;
    };

    let mut rx = tx.subscribe();
    tracing::info!("preview client connected for source {}", source_id);

    // Send FLV header
    let header = flv::flv_header();
    if socket
        .send(Message::Binary(header.to_vec().into()))
        .await
        .is_err()
    {
        return;
    }

    // Send cached sequence headers + last keyframe so clients can render immediately
    let headers = state.sequence_headers.read().await;
    if let Some(seq) = headers.get(&source_id) {
        if let Some(ref video) = seq.video {
            if socket.send(Message::Binary(video.to_vec().into())).await.is_err() {
                return;
            }
        }
        if let Some(ref audio) = seq.audio {
            if socket.send(Message::Binary(audio.to_vec().into())).await.is_err() {
                return;
            }
        }
        if let Some(ref keyframe) = seq.last_keyframe {
            if socket.send(Message::Binary(keyframe.to_vec().into())).await.is_err() {
                return;
            }
        }
    }
    drop(headers);

    let mut frame_count: u64 = 0;
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(data) => {
                        frame_count += 1;
                        if frame_count <= 5 || frame_count.is_multiple_of(300) {
                            tracing::debug!("preview frame #{} ({} bytes) for {}", frame_count, data.len(), source_id);
                        }
                        if socket.send(Message::Binary(data.to_vec().into())).await.is_err() {
                            tracing::debug!("preview send failed at frame {}", frame_count);
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("preview lagged {} messages for {}", n, source_id);
                        continue;
                    }
                    Err(e) => {
                        tracing::debug!("preview channel closed: {:?}", e);
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
