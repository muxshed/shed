// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::Response;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::hash_key;
use crate::state::AppState;
use muxshed_common::WsEvent;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let api_key = params.get("key").cloned();
    let session_token = params.get("token").cloned();
    ws.on_upgrade(move |socket| handle_socket(socket, state, api_key, session_token))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    api_key: Option<String>,
    session_token: Option<String>,
) {
    let valid = validate_ws_auth(&state, api_key.as_deref(), session_token.as_deref()).await;

    if !valid {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    // Send initial state snapshot
    let pipeline_state = state.pipeline.state().await;
    let initial = WsEvent::PipelineState {
        state: pipeline_state,
    };
    if let Ok(json) = serde_json::to_string(&initial) {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    // Send current source states
    let source_states = state.source_states.read().await;
    for (id, source_state) in source_states.iter() {
        let event = WsEvent::SourceState {
            id: *id,
            state: source_state.clone(),
        };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }
    drop(source_states);

    let mut rx = state.ws_tx.subscribe();

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
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

async fn validate_ws_auth(
    state: &AppState,
    api_key: Option<&str>,
    session_token: Option<&str>,
) -> bool {
    // Check session token first
    if let Some(token) = session_token {
        let valid: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE token = ? AND expires_at > datetime('now')",
        )
        .bind(token)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
        if valid > 0 {
            return true;
        }
    }

    // Check API key
    if let Some(key) = api_key {
        let hash = hash_key(key);
        let valid: i32 =
            sqlx::query_scalar("SELECT COUNT(*) FROM api_keys WHERE key_hash = ?")
                .bind(&hash)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);
        if valid > 0 {
            return true;
        }
    }

    false
}
