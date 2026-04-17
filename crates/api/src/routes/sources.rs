// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::media_player;
use crate::state::AppState;
use muxshed_common::{MuxshedError, Source, SourceKind, SourceState};
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct CreateSource {
    pub name: String,
    pub kind: SourceKind,
}

#[derive(Deserialize)]
pub struct UpdateSource {
    pub name: Option<String>,
    pub kind: Option<SourceKind>,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Source>>, ApiError> {
    let rows = sqlx::query_as::<_, SourceRow>(
        "SELECT id, name, kind FROM sources WHERE kind NOT LIKE '%\"media_file\"%'"
    )
    .fetch_all(&state.db)
    .await?;

    let runtime_states = state.source_states.read().await;
    let sources = rows
        .into_iter()
        .map(|r| {
            let mut source = r.into_source();
            if let Some(rs) = runtime_states.get(&source.id) {
                source.state = rs.clone();
            }
            source
        })
        .collect();
    Ok(Json(sources))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSource>,
) -> Result<(StatusCode, Json<Source>), ApiError> {
    let id = Uuid::new_v4();
    let mut kind = ensure_stream_key(body.kind);

    // Auto-assign port for SRT sources
    if let SourceKind::Srt { port, .. } = &mut kind {
        if *port == 0 {
            *port = crate::srt::assign_srt_port(&state).await;
        }
    }

    let kind_json = serde_json::to_string(&kind)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT INTO sources (id, name, kind) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(&body.name)
        .bind(&kind_json)
        .execute(&state.db)
        .await?;

    // Start SRT listener if applicable
    if let SourceKind::Srt { port, passphrase } = &kind {
        let _ = crate::srt::start_srt_listener(
            state.clone(), id, *port, passphrase.as_deref(),
        ).await;
    }

    let source = Source {
        id,
        name: body.name,
        kind,
        state: SourceState::Disconnected,
    };

    Ok((StatusCode::CREATED, Json(source)))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Source>, ApiError> {
    let row = sqlx::query_as::<_, SourceRow>("SELECT id, name, kind FROM sources WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("source {}", id)))?;

    let mut source = row.into_source();
    let runtime_states = state.source_states.read().await;
    if let Some(rs) = runtime_states.get(&source.id) {
        source.state = rs.clone();
    }
    Ok(Json(source))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSource>,
) -> Result<Json<Source>, ApiError> {
    let existing = sqlx::query_as::<_, SourceRow>("SELECT id, name, kind FROM sources WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("source {}", id)))?;

    let name = body.name.unwrap_or(existing.name);
    let kind = body.kind.map(ensure_stream_key).unwrap_or_else(|| {
        serde_json::from_str(&existing.kind).unwrap_or(SourceKind::TestPattern)
    });
    let kind_json = serde_json::to_string(&kind)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("UPDATE sources SET name = ?, kind = ? WHERE id = ?")
        .bind(&name)
        .bind(&kind_json)
        .bind(&id)
        .execute(&state.db)
        .await?;

    let source = Source {
        id: existing.id.parse().unwrap_or_default(),
        name,
        kind,
        state: SourceState::Disconnected,
    };

    Ok(Json(source))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Stop any running listener/player before deleting
    if let Ok(uid) = id.parse::<Uuid>() {
        crate::srt::stop_srt_listener(&state, &uid).await;
        crate::media_player::stop_media_playback(&state, &uid).await;
    }

    let result = sqlx::query("DELETE FROM sources WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("source {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct CreateFromAsset {
    pub asset_id: String,
    pub name: Option<String>,
}

pub async fn create_from_asset(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateFromAsset>,
) -> Result<(StatusCode, Json<Source>), ApiError> {
    let asset_row = sqlx::query_as::<_, AssetRow>(
        "SELECT id, name, file_path, loop_mode FROM assets WHERE id = ?",
    )
    .bind(&body.asset_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("asset {}", body.asset_id)))?;

    // Check if a source already exists for this asset
    let existing = sqlx::query_scalar::<_, String>("SELECT id FROM sources WHERE kind LIKE ?")
        .bind(format!("%\"asset_id\":\"{}\"%", body.asset_id))
        .fetch_optional(&state.db)
        .await?;

    if let Some(existing_id) = existing {
        let row =
            sqlx::query_as::<_, SourceRow>("SELECT id, name, kind FROM sources WHERE id = ?")
                .bind(&existing_id)
                .fetch_one(&state.db)
                .await?;
        let mut source = row.into_source();
        let runtime_states = state.source_states.read().await;
        if let Some(rs) = runtime_states.get(&source.id) {
            source.state = rs.clone();
        }
        drop(runtime_states);
        // Ensure playback is running
        let already_playing = state.media_players.read().await.contains_key(&source.id);
        if !already_playing {
            let fp = PathBuf::from(&asset_row.file_path);
            let _ = media_player::start_media_playback(
                state.clone(),
                source.id,
                &fp,
                &asset_row.loop_mode,
            )
            .await;
        }
        return Ok((StatusCode::OK, Json(source)));
    }

    let id = Uuid::new_v4();
    let source_name = body.name.unwrap_or(asset_row.name);
    let asset_uuid: Uuid = body
        .asset_id
        .parse()
        .map_err(|_| MuxshedError::BadRequest("invalid asset uuid".to_string()))?;

    let loop_mode = asset_row.loop_mode.clone();
    let kind = SourceKind::MediaFile {
        asset_id: asset_uuid,
        file_path: PathBuf::from(&asset_row.file_path),
        loop_mode: asset_row.loop_mode,
    };
    let kind_json =
        serde_json::to_string(&kind).map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT INTO sources (id, name, kind) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(&source_name)
        .bind(&kind_json)
        .execute(&state.db)
        .await?;

    // Media file sources are always "live"
    state
        .source_states
        .write()
        .await
        .insert(id, SourceState::Live);

    // Start media playback
    let _ = media_player::start_media_playback(
        state.clone(),
        id,
        &PathBuf::from(&asset_row.file_path),
        &loop_mode,
    )
    .await;

    let source = Source {
        id,
        name: source_name,
        kind,
        state: SourceState::Live,
    };

    Ok((StatusCode::CREATED, Json(source)))
}

#[derive(sqlx::FromRow)]
struct AssetRow {
    #[allow(dead_code)]
    id: String,
    name: String,
    file_path: String,
    loop_mode: String,
}

fn ensure_stream_key(kind: SourceKind) -> SourceKind {
    match kind {
        SourceKind::Rtmp { stream_key } if stream_key.is_empty() => SourceKind::Rtmp {
            stream_key: generate_stream_key(),
        },
        other => other,
    }
}

fn generate_stream_key() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..20)
        .map(|_| {
            let idx = rng.random_range(0..36u8);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct SourceRow {
    id: String,
    name: String,
    kind: String,
}

impl SourceRow {
    fn into_source(self) -> Source {
        let kind = serde_json::from_str(&self.kind).unwrap_or(SourceKind::TestPattern);
        Source {
            id: self.id.parse().unwrap_or_default(),
            name: self.name,
            kind,
            state: SourceState::Disconnected,
        }
    }
}
