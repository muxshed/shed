// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::schedule_time::{next_run_after, parse_tz};
use crate::state::AppState;
use muxshed_common::{EndBehavior, MuxshedError, Schedule, ScheduleItem, ScheduleItemKind, TriggerKind};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateItem { pub kind: ScheduleItemKind, pub ref_id: Uuid }

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpsertSchedule {
    pub name: String,
    #[serde(default = "yes")] pub enabled: bool,
    pub trigger: TriggerKind,
    #[serde(default)] pub destination_ids: Vec<Uuid>,
    pub standby_asset_id: Option<Uuid>,
    #[serde(default)] pub end_behavior: EndBehavior,
    pub until_at: Option<String>,
    #[serde(default)] pub items: Vec<CreateItem>,
}
fn yes() -> bool { true }

fn six(expr: &str) -> String {
    let p: Vec<&str> = expr.split_whitespace().collect();
    if p.len() == 5 { format!("0 {}", expr.trim()) } else { expr.trim().to_string() }
}

fn validate(u: &UpsertSchedule) -> Result<(), MuxshedError> {
    match &u.trigger {
        TriggerKind::Cron { expr } => {
            cron::Schedule::from_str(&six(expr))
                .map_err(|_| MuxshedError::BadRequest("invalid cron expression".into()))?;
        }
        TriggerKind::Once { at } => {
            chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%dT%H:%M"))
                .map_err(|_| MuxshedError::BadRequest("invalid datetime".into()))?;
        }
    }
    Ok(())
}

fn bump() -> u64 { chrono::Utc::now().timestamp_millis() as u64 }

async fn system_tz(state: &AppState) -> chrono_tz::Tz {
    let name = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='system_timezone'")
        .fetch_optional(&state.db).await.ok().flatten().map(|r| r.0).unwrap_or_else(|| "UTC".into());
    parse_tz(&name)
}

fn trigger_cols(t: &TriggerKind) -> (&'static str, Option<String>, Option<String>) {
    match t {
        TriggerKind::Once { at } => ("once", Some(at.clone()), None),
        TriggerKind::Cron { expr } => ("cron", None, Some(expr.clone())),
    }
}

/// List schedules.
#[utoipa::path(
    get,
    path = "/api/v1/schedules",
    tag = "schedules",
    responses((status = 200, description = "List of schedules", body = [muxshed_common::Schedule]))
)]
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Schedule>>, ApiError> {
    let ids = sqlx::query_as::<_, (String,)>("SELECT id FROM schedules ORDER BY created_at DESC")
        .fetch_all(&state.db).await?;
    let mut out = Vec::new();
    for (id,) in ids { if let Some(s) = load_schedule(&state, &id).await? { out.push(s); } }
    Ok(Json(out))
}

/// Create a schedule.
#[utoipa::path(
    post,
    path = "/api/v1/schedules",
    tag = "schedules",
    request_body = UpsertSchedule,
    responses((status = 201, description = "Created", body = muxshed_common::Schedule))
)]
pub async fn create(
    State(state): State<Arc<AppState>>, Json(body): Json<UpsertSchedule>,
) -> Result<(StatusCode, Json<Schedule>), ApiError> {
    validate(&body)?;
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let (kind, at, cron) = trigger_cols(&body.trigger);
    let next = next_run_after(&body.trigger, system_tz(&state).await, chrono::Utc::now()).map(|d| d.to_rfc3339());
    sqlx::query("INSERT INTO schedules \
        (id,name,enabled,trigger_kind,trigger_at,trigger_cron,destination_ids,standby_asset_id,end_behavior,until_at,next_run_at,created_at,updated_at) \
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(id.to_string()).bind(&body.name).bind(body.enabled as i64)
        .bind(kind).bind(at).bind(cron)
        .bind(serde_json::to_string(&body.destination_ids).unwrap_or_else(|_| "[]".into()))
        .bind(body.standby_asset_id.map(|x| x.to_string()))
        .bind(body.end_behavior.as_str()).bind(&body.until_at).bind(next)
        .bind(&now).bind(&now)
        .execute(&state.db).await?;
    replace_items(&state, &id, &body.items).await?;
    let _ = state.schedule_nudge.send(bump());
    let s = load_schedule(&state, &id.to_string()).await?.ok_or_else(|| MuxshedError::Internal("load".into()))?;
    Ok((StatusCode::CREATED, Json(s)))
}

/// Update a schedule.
#[utoipa::path(
    put,
    path = "/api/v1/schedules/{id}",
    tag = "schedules",
    params(("id" = String, Path, description = "Schedule id")),
    request_body = UpsertSchedule,
    responses((status = 200, description = "Updated", body = muxshed_common::Schedule))
)]
pub async fn update(
    State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(body): Json<UpsertSchedule>,
) -> Result<Json<Schedule>, ApiError> {
    validate(&body)?;
    let (kind, at, cron) = trigger_cols(&body.trigger);
    let next = next_run_after(&body.trigger, system_tz(&state).await, chrono::Utc::now()).map(|d| d.to_rfc3339());
    let res = sqlx::query("UPDATE schedules SET name=?,enabled=?,trigger_kind=?,trigger_at=?,trigger_cron=?,\
        destination_ids=?,standby_asset_id=?,end_behavior=?,until_at=?,next_run_at=?,updated_at=? WHERE id=?")
        .bind(&body.name).bind(body.enabled as i64).bind(kind).bind(at).bind(cron)
        .bind(serde_json::to_string(&body.destination_ids).unwrap_or_else(|_| "[]".into()))
        .bind(body.standby_asset_id.map(|x| x.to_string())).bind(body.end_behavior.as_str())
        .bind(&body.until_at).bind(next).bind(chrono::Utc::now().to_rfc3339()).bind(&id)
        .execute(&state.db).await?;
    if res.rows_affected() == 0 { return Err(MuxshedError::NotFound(format!("schedule {id}")).into()); }
    let sid: Uuid = id.parse().map_err(|_| MuxshedError::BadRequest("bad id".into()))?;
    replace_items(&state, &sid, &body.items).await?;
    let _ = state.schedule_nudge.send(bump());
    Ok(Json(load_schedule(&state, &id).await?.ok_or_else(|| MuxshedError::NotFound(id.clone()))?))
}

/// Delete a schedule.
#[utoipa::path(
    delete,
    path = "/api/v1/schedules/{id}",
    tag = "schedules",
    params(("id" = String, Path, description = "Schedule id")),
    responses((status = 204, description = "Deleted"))
)]
pub async fn delete(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let r = sqlx::query("DELETE FROM schedules WHERE id=?").bind(&id).execute(&state.db).await?;
    if r.rows_affected() == 0 { return Err(MuxshedError::NotFound(format!("schedule {id}")).into()); }
    let _ = state.schedule_nudge.send(bump());
    Ok(StatusCode::NO_CONTENT)
}

/// Run a schedule immediately.
#[utoipa::path(
    post,
    path = "/api/v1/schedules/{id}/run",
    tag = "schedules",
    params(("id" = String, Path, description = "Schedule id")),
    responses((status = 200, description = "Started"))
)]
pub async fn run_now(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let sid: Uuid = id.parse().map_err(|_| MuxshedError::BadRequest("bad id".into()))?;
    if state.active_schedule.borrow().is_some() {
        return Err(MuxshedError::BadRequest("already live".into()).into());
    }
    crate::playout::start_broadcast(state.clone(), sid).await;
    Ok(StatusCode::OK)
}

/// Stop the active broadcast.
#[utoipa::path(
    post,
    path = "/api/v1/schedules/stop",
    tag = "schedules",
    responses((status = 200, description = "Stopped"))
)]
pub async fn stop(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    crate::playout::stop_broadcast(&state).await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TzBody { pub timezone: String }

/// Get the system timezone.
#[utoipa::path(
    get,
    path = "/api/v1/settings/timezone",
    tag = "schedules",
    responses((status = 200, description = "Current timezone", body = TzBody))
)]
pub async fn get_timezone(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    let tz = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='system_timezone'")
        .fetch_optional(&state.db).await?.map(|r| r.0).unwrap_or_else(|| "UTC".into());
    Ok(Json(serde_json::json!({ "timezone": tz })))
}

/// Set the system timezone.
#[utoipa::path(
    put,
    path = "/api/v1/settings/timezone",
    tag = "schedules",
    request_body = TzBody,
    responses((status = 200, description = "Updated timezone", body = TzBody))
)]
pub async fn set_timezone(State(state): State<Arc<AppState>>, Json(b): Json<TzBody>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(MuxshedError::BadRequest("unknown timezone".into()).into());
    }
    sqlx::query("INSERT OR REPLACE INTO settings (key,value) VALUES ('system_timezone', ?)")
        .bind(&b.timezone).execute(&state.db).await?;
    recompute_all(&state).await?;
    let _ = state.schedule_nudge.send(bump());
    Ok(Json(serde_json::json!({ "timezone": b.timezone })))
}

async fn replace_items(state: &AppState, schedule_id: &Uuid, items: &[CreateItem]) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM schedule_items WHERE schedule_id=?").bind(schedule_id.to_string()).execute(&state.db).await?;
    for (i, it) in items.iter().enumerate() {
        sqlx::query("INSERT INTO schedule_items (id,schedule_id,position,item_kind,ref_id) VALUES (?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(schedule_id.to_string())
            .bind(i as i64).bind(it.kind.as_str()).bind(it.ref_id.to_string())
            .execute(&state.db).await?;
    }
    Ok(())
}

async fn recompute_all(state: &AppState) -> Result<(), ApiError> {
    let tz = system_tz(state).await;
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT id,trigger_kind,trigger_at,trigger_cron FROM schedules WHERE enabled=1").fetch_all(&state.db).await?;
    for (id, kind, at, cron) in rows {
        let t = if kind == "cron" { TriggerKind::Cron { expr: cron.unwrap_or_default() } }
                else { TriggerKind::Once { at: at.unwrap_or_default() } };
        let next = next_run_after(&t, tz, chrono::Utc::now()).map(|d| d.to_rfc3339());
        sqlx::query("UPDATE schedules SET next_run_at=? WHERE id=?").bind(next).bind(id).execute(&state.db).await?;
    }
    Ok(())
}

async fn load_schedule(state: &AppState, id: &str) -> Result<Option<Schedule>, ApiError> {
    let row = sqlx::query_as::<_, (String,String,i64,String,Option<String>,Option<String>,String,Option<String>,String,Option<String>,Option<String>)>(
        "SELECT id,name,enabled,trigger_kind,trigger_at,trigger_cron,destination_ids,standby_asset_id,end_behavior,until_at,next_run_at FROM schedules WHERE id=?")
        .bind(id).fetch_optional(&state.db).await?;
    let Some(r) = row else { return Ok(None) };
    let items = sqlx::query_as::<_, (String,i64,String,String)>(
        "SELECT id,position,item_kind,ref_id FROM schedule_items WHERE schedule_id=? ORDER BY position")
        .bind(id).fetch_all(&state.db).await?
        .into_iter().map(|(iid,pos,k,ref_id)| ScheduleItem {
            id: iid.parse().unwrap_or_default(), position: pos as u32,
            kind: ScheduleItemKind::from_db(&k), ref_id: ref_id.parse().unwrap_or_default(),
        }).collect();
    let trigger = if r.3 == "cron" { TriggerKind::Cron { expr: r.5.unwrap_or_default() } }
                  else { TriggerKind::Once { at: r.4.unwrap_or_default() } };
    Ok(Some(Schedule {
        id: r.0.parse().unwrap_or_default(), name: r.1, enabled: r.2 != 0, trigger,
        destination_ids: serde_json::from_str(&r.6).unwrap_or_default(),
        standby_asset_id: r.7.and_then(|s| s.parse().ok()),
        end_behavior: EndBehavior::from_db(&r.8), until_at: r.9,
        items, next_run_at: r.10,
    }))
}
