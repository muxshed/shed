// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{MuxshedError, Overlay, OverlayKind, Position, Size};

#[derive(Deserialize)]
pub struct CreateOverlay {
    pub name: String,
    pub kind: OverlayKind,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub z_index: Option<u32>,
}

#[derive(Deserialize)]
pub struct UpdateOverlay {
    pub name: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub visible: Option<bool>,
    pub z_index: Option<u32>,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Overlay>>, ApiError> {
    let rows = sqlx::query_as::<_, OverlayRow>(
        "SELECT id, name, kind, x, y, width, height, visible, z_index FROM overlays",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_overlay()).collect()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateOverlay>,
) -> Result<(StatusCode, Json<Overlay>), ApiError> {
    let id = Uuid::new_v4();
    let kind_json =
        serde_json::to_string(&body.kind).map_err(|e| MuxshedError::Internal(e.to_string()))?;
    let x = body.x.unwrap_or(0);
    let y = body.y.unwrap_or(0);
    let width = body.width.unwrap_or(0) as i32;
    let height = body.height.unwrap_or(0) as i32;
    let z_index = body.z_index.unwrap_or(10) as i32;

    sqlx::query(
        "INSERT INTO overlays (id, name, kind, x, y, width, height, z_index) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&body.name)
    .bind(&kind_json)
    .bind(x)
    .bind(y)
    .bind(width)
    .bind(height)
    .bind(z_index)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(Overlay {
            id,
            name: body.name,
            kind: body.kind,
            position: Position { x, y },
            size: Size {
                width: width as u32,
                height: height as u32,
            },
            visible: false,
            z_index: z_index as u32,
        }),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateOverlay>,
) -> Result<Json<Overlay>, ApiError> {
    let existing = sqlx::query_as::<_, OverlayRow>(
        "SELECT id, name, kind, x, y, width, height, visible, z_index FROM overlays WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("overlay {}", id)))?;

    let name = body.name.unwrap_or(existing.name);
    let x = body.x.unwrap_or(existing.x);
    let y = body.y.unwrap_or(existing.y);
    let width = body.width.map(|w| w as i32).unwrap_or(existing.width);
    let height = body.height.map(|h| h as i32).unwrap_or(existing.height);
    let visible = body.visible.map(|v| v as i32).unwrap_or(existing.visible);
    let z_index = body.z_index.map(|z| z as i32).unwrap_or(existing.z_index);

    sqlx::query(
        "UPDATE overlays SET name = ?, x = ?, y = ?, width = ?, height = ?, visible = ?, z_index = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(x)
    .bind(y)
    .bind(width)
    .bind(height)
    .bind(visible)
    .bind(z_index)
    .bind(&id)
    .execute(&state.db)
    .await?;

    let kind = serde_json::from_str(&existing.kind).unwrap_or(OverlayKind::Image {
        file_path: std::path::PathBuf::new(),
    });

    Ok(Json(Overlay {
        id: id.parse().unwrap_or_default(),
        name,
        kind,
        position: Position { x, y },
        size: Size {
            width: width as u32,
            height: height as u32,
        },
        visible: visible != 0,
        z_index: z_index as u32,
    }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM overlays WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("overlay {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn show(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_visible(&state, &id, true).await
}

pub async fn hide(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_visible(&state, &id, false).await
}

async fn set_visible(state: &Arc<AppState>, id: &str, visible: bool) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("UPDATE overlays SET visible = ? WHERE id = ?")
        .bind(visible as i32)
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("overlay {}", id)).into());
    }

    Ok(StatusCode::OK)
}

#[derive(sqlx::FromRow)]
struct OverlayRow {
    id: String,
    name: String,
    kind: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    visible: i32,
    z_index: i32,
}

impl OverlayRow {
    fn into_overlay(self) -> Overlay {
        let kind = serde_json::from_str(&self.kind).unwrap_or(OverlayKind::Image {
            file_path: std::path::PathBuf::new(),
        });
        Overlay {
            id: self.id.parse().unwrap_or_default(),
            name: self.name,
            kind,
            position: Position {
                x: self.x,
                y: self.y,
            },
            size: Size {
                width: self.width as u32,
                height: self.height as u32,
            },
            visible: self.visible != 0,
            z_index: self.z_index as u32,
        }
    }
}
