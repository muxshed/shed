// Licensed under the Business Source License 1.1 — see LICENSE.

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{MuxshedError, StingerAudio, StingerConfig};

#[derive(Deserialize)]
pub struct CreateStinger {
    pub name: String,
    pub file_path: String,
    pub duration_ms: Option<u64>,
    pub start_ms: Option<u64>,
    pub opaque_ms: Option<u64>,
    pub clear_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub audio_behaviour: Option<StingerAudio>,
}

#[derive(Deserialize)]
pub struct UpdateStinger {
    pub name: Option<String>,
    pub start_ms: Option<u64>,
    pub opaque_ms: Option<u64>,
    pub clear_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub audio_behaviour: Option<StingerAudio>,
}

#[derive(Deserialize)]
pub struct TriggerStinger {
    pub stinger_id: Uuid,
    pub target_scene_id: Uuid,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<StingerConfig>>, ApiError> {
    let rows = sqlx::query_as::<_, StingerRow>(
        "SELECT id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path FROM stingers",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_config()).collect()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateStinger>,
) -> Result<(StatusCode, Json<StingerConfig>), ApiError> {
    let id = Uuid::new_v4();
    let audio = body.audio_behaviour.unwrap_or(StingerAudio::Silent);
    let audio_json =
        serde_json::to_string(&audio).map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO stingers (id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&body.name)
    .bind(&body.file_path)
    .bind(body.duration_ms.unwrap_or(0) as i64)
    .bind(body.start_ms.unwrap_or(0) as i64)
    .bind(body.opaque_ms.unwrap_or(0) as i64)
    .bind(body.clear_ms.unwrap_or(0) as i64)
    .bind(body.end_ms.unwrap_or(0) as i64)
    .bind(&audio_json)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(StingerConfig {
            id,
            name: body.name,
            file_path: PathBuf::from(&body.file_path),
            duration_ms: body.duration_ms.unwrap_or(0),
            start_ms: body.start_ms.unwrap_or(0),
            opaque_ms: body.opaque_ms.unwrap_or(0),
            clear_ms: body.clear_ms.unwrap_or(0),
            end_ms: body.end_ms.unwrap_or(0),
            audio_behaviour: audio,
            thumbnail_path: PathBuf::new(),
        }),
    ))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<StingerConfig>, ApiError> {
    let row = sqlx::query_as::<_, StingerRow>(
        "SELECT id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path FROM stingers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("stinger {}", id)))?;

    Ok(Json(row.into_config()))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStinger>,
) -> Result<Json<StingerConfig>, ApiError> {
    let existing = sqlx::query_as::<_, StingerRow>(
        "SELECT id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path FROM stingers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("stinger {}", id)))?;

    let name = body.name.unwrap_or(existing.name.clone());
    let start_ms = body.start_ms.unwrap_or(existing.start_ms as u64);
    let opaque_ms = body.opaque_ms.unwrap_or(existing.opaque_ms as u64);
    let clear_ms = body.clear_ms.unwrap_or(existing.clear_ms as u64);
    let end_ms = body.end_ms.unwrap_or(existing.end_ms as u64);
    let audio = body.audio_behaviour.unwrap_or_else(|| {
        serde_json::from_str(&existing.audio_behaviour).unwrap_or(StingerAudio::Silent)
    });
    let audio_json =
        serde_json::to_string(&audio).map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query(
        "UPDATE stingers SET name = ?, start_ms = ?, opaque_ms = ?, clear_ms = ?, end_ms = ?, audio_behaviour = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(start_ms as i64)
    .bind(opaque_ms as i64)
    .bind(clear_ms as i64)
    .bind(end_ms as i64)
    .bind(&audio_json)
    .bind(&id)
    .execute(&state.db)
    .await?;

    Ok(Json(StingerConfig {
        id: id.parse().unwrap_or_default(),
        name,
        file_path: PathBuf::from(&existing.file_path),
        duration_ms: existing.duration_ms as u64,
        start_ms,
        opaque_ms,
        clear_ms,
        end_ms,
        audio_behaviour: audio,
        thumbnail_path: PathBuf::from(&existing.thumbnail_path),
    }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM stingers WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("stinger {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn trigger(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TriggerStinger>,
) -> Result<StatusCode, ApiError> {
    // Verify stinger exists
    let _stinger = sqlx::query_as::<_, StingerRow>(
        "SELECT id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour, thumbnail_path FROM stingers WHERE id = ?",
    )
    .bind(body.stinger_id.to_string())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("stinger {}", body.stinger_id)))?;

    state
        .pipeline
        .trigger_stinger_transition(&body.stinger_id, &body.target_scene_id)
        .await?;

    Ok(StatusCode::OK)
}

pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<StingerConfig>), ApiError> {
    let mut name: Option<String> = None;
    let mut file_data: Option<(String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MuxshedError::BadRequest(e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "name" => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| MuxshedError::BadRequest(e.to_string()))?,
                );
            }
            "file" => {
                let filename = field
                    .file_name()
                    .unwrap_or("upload.webm")
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| MuxshedError::BadRequest(e.to_string()))?;
                file_data = Some((filename, data.to_vec()));
            }
            _ => {}
        }
    }

    let (filename, data) = file_data
        .ok_or_else(|| MuxshedError::BadRequest("no file provided".to_string()))?;
    let name = name.unwrap_or_else(|| filename.clone());

    // Save to library directory
    let config = state.config.read().await;
    let library_dir = config.data_dir.join("library");
    drop(config);

    tokio::fs::create_dir_all(&library_dir)
        .await
        .map_err(|e| MuxshedError::Internal(format!("failed to create library dir: {}", e)))?;

    let id = Uuid::new_v4();
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("webm");
    let stored_name = format!("{}_{}.{}", id, sanitize_filename(&name), ext);
    let file_path = library_dir.join(&stored_name);

    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| MuxshedError::Internal(format!("failed to create file: {}", e)))?;
    file.write_all(&data)
        .await
        .map_err(|e| MuxshedError::Internal(format!("failed to write file: {}", e)))?;

    let file_path_str = file_path.display().to_string();
    let audio_json = serde_json::to_string(&StingerAudio::Silent)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query(
        "INSERT INTO stingers (id, name, file_path, duration_ms, start_ms, opaque_ms, clear_ms, end_ms, audio_behaviour) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(&file_path_str)
    .bind(0i64)
    .bind(0i64)
    .bind(0i64)
    .bind(0i64)
    .bind(0i64)
    .bind(&audio_json)
    .execute(&state.db)
    .await?;

    tracing::info!("uploaded library item: {} → {}", name, file_path_str);

    Ok((
        StatusCode::CREATED,
        Json(StingerConfig {
            id,
            name,
            file_path,
            duration_ms: 0,
            start_ms: 0,
            opaque_ms: 0,
            clear_ms: 0,
            end_ms: 0,
            audio_behaviour: StingerAudio::Silent,
            thumbnail_path: PathBuf::new(),
        }),
    ))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}

#[derive(sqlx::FromRow)]
struct StingerRow {
    id: String,
    name: String,
    file_path: String,
    duration_ms: i64,
    start_ms: i64,
    opaque_ms: i64,
    clear_ms: i64,
    end_ms: i64,
    audio_behaviour: String,
    thumbnail_path: String,
}

impl StingerRow {
    fn into_config(self) -> StingerConfig {
        StingerConfig {
            id: self.id.parse().unwrap_or_default(),
            name: self.name,
            file_path: PathBuf::from(self.file_path),
            duration_ms: self.duration_ms as u64,
            start_ms: self.start_ms as u64,
            opaque_ms: self.opaque_ms as u64,
            clear_ms: self.clear_ms as u64,
            end_ms: self.end_ms as u64,
            audio_behaviour: serde_json::from_str(&self.audio_behaviour)
                .unwrap_or(StingerAudio::Silent),
            thumbnail_path: PathBuf::from(self.thumbnail_path),
        }
    }
}
