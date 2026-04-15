// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use muxshed_common::{MuxshedConfig, WsEvent};
use muxshed_processor::StubPipelineController;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use tracing_subscriber::EnvFilter;

use muxshed_api::egress::EgressManager;
use muxshed_api::program::run_program_router;
use muxshed_api::routes;
use muxshed_api::rtmp::start_rtmp_server;
use muxshed_api::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = MuxshedConfig::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let db_url = format!("sqlite:{}?mode=rwc", config.db_path.display());
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::migrate!("../../migrations").run(&db).await?;

    let needs_setup: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&db)
        .await?;
    if needs_setup == 0 {
        tracing::info!("no API keys found — open the web UI to complete setup");
    }

    let (ws_tx, _) = broadcast::channel::<WsEvent>(256);
    let (program_tx, _) = broadcast::channel::<bytes::Bytes>(4096);
    let (program_source_tx, _program_source_rx) = watch::channel::<Option<uuid::Uuid>>(None);
    // Restore saved audio routing
    let saved_audio_routing: muxshed_api::state::AudioRouting = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM settings WHERE key = 'audio_routing'",
    )
    .fetch_optional(&db)
    .await
    .ok()
    .flatten()
    .and_then(|(json,)| serde_json::from_str(&json).ok())
    .unwrap_or_default();
    let (audio_routing_tx, _audio_routing_rx) = watch::channel(saved_audio_routing);
    let pipeline = Arc::new(StubPipelineController::new(ws_tx.clone()));

    let state = Arc::new(AppState {
        pipeline,
        config: Arc::new(RwLock::new(config.clone())),
        db,
        egress: EgressManager::new(ws_tx.clone()),
        ws_tx,
        source_states: RwLock::new(std::collections::HashMap::new()),
        media_relays: RwLock::new(std::collections::HashMap::new()),
        sequence_headers: RwLock::new(std::collections::HashMap::new()),
        source_media_info: RwLock::new(std::collections::HashMap::new()),
        media_players: RwLock::new(std::collections::HashMap::new()),
        source_normalizers: RwLock::new(std::collections::HashMap::new()),
        srt_listeners: RwLock::new(std::collections::HashMap::new()),
        program_tx,
        program_source: program_source_tx,
        preview_source: RwLock::new(None),
        audio_routing: audio_routing_tx,
    });

    // Set media file sources as Live on startup and start their playback
    {
        let rows = sqlx::query_as::<_, (String, String)>("SELECT id, kind FROM sources")
            .fetch_all(&state.db)
            .await?;
        let mut states = state.source_states.write().await;
        for (id_str, kind_json) in &rows {
            if kind_json.contains("\"media_file\"") {
                if let Ok(id) = id_str.parse::<uuid::Uuid>() {
                    states.insert(id, muxshed_common::SourceState::Live);
                }
            }
        }
        drop(states);
        for (id_str, kind_json) in rows {
            if let Ok(kind) = serde_json::from_str::<muxshed_common::SourceKind>(&kind_json) {
                if let muxshed_common::SourceKind::MediaFile { file_path, loop_mode, .. } = kind {
                    if let Ok(id) = id_str.parse::<uuid::Uuid>() {
                        let s = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = muxshed_api::media_player::start_media_playback(
                                s, id, &file_path, &loop_mode,
                            ).await {
                                tracing::warn!("failed to start media playback for {}: {}", id, e);
                            }
                        });
                    }
                } else if let muxshed_common::SourceKind::Srt { port, passphrase } = kind {
                    if let Ok(id) = id_str.parse::<uuid::Uuid>() {
                        let s = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = muxshed_api::srt::start_srt_listener(
                                s, id, port, passphrase.as_deref(),
                            ).await {
                                tracing::warn!("failed to start SRT listener for {}: {}", id, e);
                            }
                        });
                    }
                }
            }
        }
    }

    // Start program router (forwards active source to program output)
    let program_state = state.clone();
    tokio::spawn(async move {
        run_program_router(program_state).await;
    });

    // Start RTMP ingest server
    let rtmp_state = state.clone();
    let rtmp_port = config.rtmp_port;
    tokio::spawn(async move {
        start_rtmp_server(rtmp_state, rtmp_port).await;
    });

    let app = routes::build_router(state.clone(), config.web_dir.clone());

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
