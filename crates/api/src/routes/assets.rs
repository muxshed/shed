// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::MuxshedError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub asset_type: String,
    pub file_path: String,
    pub file_size: i64,
    pub mime_type: String,
    pub folder_id: Option<String>,
    pub loop_mode: String,
    pub duration_ms: i64,
    pub start_ms: i64,
    pub opaque_ms: i64,
    pub clear_ms: i64,
    pub end_ms: i64,
    pub audio_behaviour: String,
    pub has_thumbnail: bool,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct UpdateAsset {
    pub name: Option<String>,
    pub folder_id: Option<Option<String>>,
    pub loop_mode: Option<String>,
    pub duration_ms: Option<i64>,
    pub start_ms: Option<i64>,
    pub opaque_ms: Option<i64>,
    pub clear_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub audio_behaviour: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateFolder {
    pub name: String,
    pub parent_id: Option<String>,
    pub color: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateFolder {
    pub name: Option<String>,
    pub color: Option<String>,
}

// --- Assets ---

pub async fn list_assets(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Asset>>, ApiError> {
    let query = if let Some(folder_id) = params.get("folder_id") {
        if folder_id == "none" {
            "SELECT * FROM assets WHERE folder_id IS NULL ORDER BY asset_type, name".to_string()
        } else {
            format!(
                "SELECT * FROM assets WHERE folder_id = '{}' ORDER BY asset_type, name",
                folder_id.replace('\'', "")
            )
        }
    } else {
        "SELECT * FROM assets ORDER BY asset_type, name".to_string()
    };

    let rows = sqlx::query_as::<_, AssetRow>(&query)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_asset()).collect()))
}

pub async fn get_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Asset>, ApiError> {
    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("asset {}", id)))?;

    Ok(Json(row.into_asset()))
}

pub async fn update_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAsset>,
) -> Result<Json<Asset>, ApiError> {
    let existing = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("asset {}", id)))?;

    let name = body.name.unwrap_or(existing.name.clone());
    let folder_id = body.folder_id.unwrap_or(existing.folder_id.clone());
    let loop_mode = body.loop_mode.unwrap_or(existing.loop_mode.clone());
    let duration_ms = body.duration_ms.unwrap_or(existing.duration_ms);
    let start_ms = body.start_ms.unwrap_or(existing.start_ms);
    let opaque_ms = body.opaque_ms.unwrap_or(existing.opaque_ms);
    let clear_ms = body.clear_ms.unwrap_or(existing.clear_ms);
    let end_ms = body.end_ms.unwrap_or(existing.end_ms);
    let audio_behaviour = body.audio_behaviour.unwrap_or(existing.audio_behaviour.clone());

    sqlx::query(
        "UPDATE assets SET name=?, folder_id=?, loop_mode=?, duration_ms=?, start_ms=?, opaque_ms=?, clear_ms=?, end_ms=?, audio_behaviour=? WHERE id=?",
    )
    .bind(&name)
    .bind(&folder_id)
    .bind(&loop_mode)
    .bind(duration_ms)
    .bind(start_ms)
    .bind(opaque_ms)
    .bind(clear_ms)
    .bind(end_ms)
    .bind(&audio_behaviour)
    .bind(&id)
    .execute(&state.db)
    .await?;

    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(row.into_asset()))
}

pub async fn delete_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Get file path to delete from disk
    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?;

    if let Some(row) = row {
        let _ = tokio::fs::remove_file(&row.file_path).await;
    }

    let result = sqlx::query("DELETE FROM assets WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("asset {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn upload_asset(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Asset>), ApiError> {
    let mut name: Option<String> = None;
    let mut folder_id: Option<String> = None;
    let mut file_data: Option<(String, Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MuxshedError::BadRequest(e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "name" => {
                name = Some(field.text().await.map_err(|e| MuxshedError::BadRequest(e.to_string()))?);
            }
            "folder_id" => {
                let val = field.text().await.map_err(|e| MuxshedError::BadRequest(e.to_string()))?;
                if !val.is_empty() {
                    folder_id = Some(val);
                }
            }
            "file" => {
                let filename = field.file_name().unwrap_or("upload").to_string();
                let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
                let data = field.bytes().await.map_err(|e| MuxshedError::BadRequest(e.to_string()))?;
                file_data = Some((filename, data.to_vec(), content_type));
            }
            _ => {}
        }
    }

    let (filename, data, content_type) =
        file_data.ok_or_else(|| MuxshedError::BadRequest("no file provided".to_string()))?;
    let file_size = data.len() as i64;
    let name = name.unwrap_or_else(|| filename.clone().replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.', "_"));

    // Determine asset type from mime/extension
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let asset_type = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "image",
        "webm" | "mov" => "stinger",
        "mp4" | "mkv" | "avi" => "video",
        _ => {
            if content_type.starts_with("image/") {
                "image"
            } else {
                "video"
            }
        }
    };

    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp4" => "video/mp4",
        _ => &content_type,
    };

    // Save file
    let config = state.config.read().await;
    let library_dir = config.data_dir.join("library");
    drop(config);

    tokio::fs::create_dir_all(&library_dir)
        .await
        .map_err(|e| MuxshedError::Internal(format!("create dir: {}", e)))?;

    let id = Uuid::new_v4();
    let _safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(50)
        .collect();
    let stored = format!("{}.{}", id, ext);
    let file_path = library_dir.join(&stored);

    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| MuxshedError::Internal(format!("create file: {}", e)))?;
    file.write_all(&data)
        .await
        .map_err(|e| MuxshedError::Internal(format!("write file: {}", e)))?;

    let file_path_str = file_path.display().to_string();

    // Probe media file for real metadata
    let probe = crate::media_probe::probe_file(&file_path).await;
    let duration_ms = probe.as_ref().map(|p| p.duration_ms as i64).unwrap_or(0);
    let metadata = serde_json::to_string(&probe.unwrap_or_default()).unwrap_or_else(|_| "{}".to_string());

    // Generate thumbnail for video files
    let thumb_dir = library_dir.join("thumbnails");
    let _ = tokio::fs::create_dir_all(&thumb_dir).await;
    let thumbnail_path = if asset_type != "image" {
        crate::media_probe::generate_thumbnail(&file_path, &thumb_dir, &id.to_string())
            .await
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    sqlx::query(
        "INSERT INTO assets (id, name, asset_type, file_path, file_size, mime_type, folder_id, loop_mode, duration_ms, thumbnail_path, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, 'one_shot', ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&name)
    .bind(asset_type)
    .bind(&file_path_str)
    .bind(file_size)
    .bind(mime)
    .bind(&folder_id)
    .bind(duration_ms)
    .bind(&thumbnail_path)
    .bind(&metadata)
    .execute(&state.db)
    .await?;

    tracing::info!("uploaded asset: {} ({}) {}ms → {}", name, asset_type, duration_ms, file_path_str);

    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;

    Ok((StatusCode::CREATED, Json(row.into_asset())))
}

// --- Folders ---

pub async fn list_folders(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Folder>>, ApiError> {
    let rows = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders ORDER BY name")
        .fetch_all(&state.db)
        .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_folder()).collect()))
}

pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateFolder>,
) -> Result<(StatusCode, Json<Folder>), ApiError> {
    let id = Uuid::new_v4().to_string();
    let color = body.color.unwrap_or_else(|| "#6366f1".to_string());

    sqlx::query("INSERT INTO folders (id, name, parent_id, color) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&body.name)
        .bind(&body.parent_id)
        .bind(&color)
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    Ok((StatusCode::CREATED, Json(row.into_folder())))
}

pub async fn update_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateFolder>,
) -> Result<Json<Folder>, ApiError> {
    let existing = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("folder {}", id)))?;

    let name = body.name.unwrap_or(existing.name);
    let color = body.color.unwrap_or(existing.color);

    sqlx::query("UPDATE folders SET name = ?, color = ? WHERE id = ?")
        .bind(&name)
        .bind(&color)
        .bind(&id)
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, FolderRow>("SELECT * FROM folders WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(row.into_folder()))
}

pub async fn delete_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Move assets in this folder to root
    sqlx::query("UPDATE assets SET folder_id = NULL WHERE folder_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    let result = sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("folder {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Serve asset files ---

pub async fn serve_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("asset {}", id)))?;

    let data = tokio::fs::read(&row.file_path)
        .await
        .map_err(|e| MuxshedError::Internal(format!("read file: {}", e)))?;

    Ok(axum::response::Response::builder()
        .header("Content-Type", &row.mime_type)
        .header("Cache-Control", "public, max-age=86400")
        .body(axum::body::Body::from(data))
        .unwrap())
}

/// Serve asset thumbnail
pub async fn serve_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let row = sqlx::query_as::<_, AssetRow>("SELECT * FROM assets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("asset {}", id)))?;

    if row.thumbnail_path.is_empty() {
        return Err(MuxshedError::NotFound("no thumbnail".to_string()).into());
    }

    let data = tokio::fs::read(&row.thumbnail_path)
        .await
        .map_err(|e| MuxshedError::Internal(format!("read thumbnail: {}", e)))?;

    Ok(axum::response::Response::builder()
        .header("Content-Type", "image/jpeg")
        .header("Cache-Control", "public, max-age=86400")
        .body(axum::body::Body::from(data))
        .unwrap())
}

// --- Row types ---

#[derive(sqlx::FromRow)]
struct AssetRow {
    id: String,
    name: String,
    asset_type: String,
    file_path: String,
    file_size: i64,
    mime_type: String,
    folder_id: Option<String>,
    loop_mode: String,
    duration_ms: i64,
    start_ms: i64,
    opaque_ms: i64,
    clear_ms: i64,
    end_ms: i64,
    audio_behaviour: String,
    thumbnail_path: String,
    metadata: String,
    created_at: String,
}

impl AssetRow {
    fn into_asset(self) -> Asset {
        Asset {
            id: self.id,
            name: self.name,
            asset_type: self.asset_type,
            file_path: self.file_path,
            file_size: self.file_size,
            mime_type: self.mime_type,
            folder_id: self.folder_id,
            loop_mode: self.loop_mode,
            duration_ms: self.duration_ms,
            start_ms: self.start_ms,
            opaque_ms: self.opaque_ms,
            clear_ms: self.clear_ms,
            end_ms: self.end_ms,
            audio_behaviour: self.audio_behaviour,
            has_thumbnail: !self.thumbnail_path.is_empty(),
            metadata: serde_json::from_str(&self.metadata).unwrap_or(serde_json::Value::Object(Default::default())),
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id: String,
    name: String,
    parent_id: Option<String>,
    color: String,
    created_at: String,
}

impl FolderRow {
    fn into_folder(self) -> Folder {
        Folder {
            id: self.id,
            name: self.name,
            parent_id: self.parent_id,
            color: self.color,
            created_at: self.created_at,
        }
    }
}
