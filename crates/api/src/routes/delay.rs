// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::DelayConfig;

pub async fn get_delay(State(state): State<Arc<AppState>>) -> Result<Json<DelayConfig>, ApiError> {
    let row = sqlx::query_as::<_, DelayRow>(
        "SELECT enabled, duration_ms, whisper_enabled FROM delay_config WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(DelayConfig {
        enabled: row.enabled != 0,
        duration_ms: row.duration_ms as u64,
        whisper_enabled: row.whisper_enabled != 0,
    }))
}

#[derive(Deserialize)]
pub struct UpdateDelay {
    pub enabled: Option<bool>,
    pub duration_ms: Option<u64>,
    pub whisper_enabled: Option<bool>,
}

pub async fn update_delay(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateDelay>,
) -> Result<Json<DelayConfig>, ApiError> {
    let existing = sqlx::query_as::<_, DelayRow>(
        "SELECT enabled, duration_ms, whisper_enabled FROM delay_config WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    let enabled = body.enabled.unwrap_or(existing.enabled != 0);
    let duration_ms = body.duration_ms.unwrap_or(existing.duration_ms as u64);
    let whisper_enabled = body.whisper_enabled.unwrap_or(existing.whisper_enabled != 0);

    sqlx::query(
        "UPDATE delay_config SET enabled = ?, duration_ms = ?, whisper_enabled = ? WHERE id = 1",
    )
    .bind(enabled as i32)
    .bind(duration_ms as i64)
    .bind(whisper_enabled as i32)
    .execute(&state.db)
    .await?;

    let config = DelayConfig {
        enabled,
        duration_ms,
        whisper_enabled,
    };

    state.pipeline.set_delay(&config).await?;

    Ok(Json(config))
}

pub async fn enable(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    sqlx::query("UPDATE delay_config SET enabled = 1 WHERE id = 1")
        .execute(&state.db)
        .await?;

    let row = sqlx::query_as::<_, DelayRow>(
        "SELECT enabled, duration_ms, whisper_enabled FROM delay_config WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    let config = DelayConfig {
        enabled: true,
        duration_ms: row.duration_ms as u64,
        whisper_enabled: row.whisper_enabled != 0,
    };
    state.pipeline.set_delay(&config).await?;

    Ok(StatusCode::OK)
}

pub async fn disable(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    sqlx::query("UPDATE delay_config SET enabled = 0 WHERE id = 1")
        .execute(&state.db)
        .await?;

    let config = DelayConfig {
        enabled: false,
        duration_ms: 0,
        whisper_enabled: false,
    };
    state.pipeline.set_delay(&config).await?;

    Ok(StatusCode::OK)
}

pub async fn bleep(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    state.pipeline.trigger_bleep().await?;
    Ok(StatusCode::OK)
}

#[derive(sqlx::FromRow)]
struct DelayRow {
    enabled: i32,
    duration_ms: i64,
    whisper_enabled: i32,
}
