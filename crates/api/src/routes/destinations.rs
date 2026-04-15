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
use muxshed_common::{Destination, DestinationKind, MuxshedError, WsEvent};

#[derive(Deserialize)]
pub struct CreateDestination {
    pub name: String,
    pub kind: DestinationKind,
}

#[derive(Deserialize)]
pub struct UpdateDestination {
    pub name: Option<String>,
    pub kind: Option<DestinationKind>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Destination>>, ApiError> {
    let rows = sqlx::query_as::<_, DestRow>("SELECT id, name, kind, enabled FROM destinations")
        .fetch_all(&state.db)
        .await?;

    let dests = rows.into_iter().map(|r| r.into_destination()).collect();
    Ok(Json(dests))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateDestination>,
) -> Result<(StatusCode, Json<Destination>), ApiError> {
    let id = Uuid::new_v4();
    let kind_json = serde_json::to_string(&body.kind)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("INSERT INTO destinations (id, name, kind) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(&body.name)
        .bind(&kind_json)
        .execute(&state.db)
        .await?;

    let dest = Destination {
        id,
        name: body.name,
        kind: body.kind,
        enabled: true,
    };

    Ok((StatusCode::CREATED, Json(dest)))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateDestination>,
) -> Result<Json<Destination>, ApiError> {
    let existing =
        sqlx::query_as::<_, DestRow>("SELECT id, name, kind, enabled FROM destinations WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| MuxshedError::NotFound(format!("destination {}", id)))?;

    let name = body.name.unwrap_or(existing.name);
    let kind = body.kind.unwrap_or_else(|| {
        serde_json::from_str(&existing.kind)
            .unwrap_or(DestinationKind::Rtmp {
                url: String::new(),
                stream_key: String::new(),
            })
    });
    let kind_json = serde_json::to_string(&kind)
        .map_err(|e| MuxshedError::Internal(e.to_string()))?;

    sqlx::query("UPDATE destinations SET name = ?, kind = ? WHERE id = ?")
        .bind(&name)
        .bind(&kind_json)
        .bind(&id)
        .execute(&state.db)
        .await?;

    let dest = Destination {
        id: existing.id.parse().unwrap_or_default(),
        name,
        kind,
        enabled: existing.enabled != 0,
    };

    Ok(Json(dest))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM destinations WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("destination {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_enabled(&state, &id, true).await
}

pub async fn disable(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    set_enabled(&state, &id, false).await
}

async fn set_enabled(
    state: &Arc<AppState>,
    id: &str,
    enabled: bool,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("UPDATE destinations SET enabled = ? WHERE id = ?")
        .bind(enabled as i32)
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("destination {}", id)).into());
    }

    let _ = state.ws_tx.send(WsEvent::DestinationState {
        id: id.parse().unwrap_or_default(),
        state: if enabled {
            "enabled".to_string()
        } else {
            "disabled".to_string()
        },
    });

    Ok(StatusCode::OK)
}

#[derive(sqlx::FromRow)]
struct DestRow {
    id: String,
    name: String,
    kind: String,
    enabled: i32,
}

impl DestRow {
    fn into_destination(self) -> Destination {
        let kind = serde_json::from_str(&self.kind).unwrap_or(DestinationKind::Rtmp {
            url: String::new(),
            stream_key: String::new(),
        });
        Destination {
            id: self.id.parse().unwrap_or_default(),
            name: self.name,
            kind,
            enabled: self.enabled != 0,
        }
    }
}
