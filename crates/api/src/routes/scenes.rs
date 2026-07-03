// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use muxshed_common::{Layer, LayerFit, MuxshedError, Position, Scene, Size, WsEvent};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateScene {
    pub name: String,
    pub layers: Option<Vec<CreateLayer>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateScene {
    pub name: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateLayer {
    pub source_id: Uuid,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub z_index: Option<u32>,
    pub opacity: Option<f32>,
    pub fit: Option<LayerFit>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateLayer {
    pub source_id: Option<Uuid>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub z_index: Option<u32>,
    pub opacity: Option<f32>,
    pub fit: Option<LayerFit>,
}

/// List all scenes.
#[utoipa::path(
    get,
    path = "/api/v1/scenes",
    tag = "scenes",
    responses((status = 200, description = "Scenes", body = [muxshed_common::Scene]))
)]
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Scene>>, ApiError> {
    let scene_rows = sqlx::query_as::<_, SceneRow>("SELECT id, name FROM scenes")
        .fetch_all(&state.db)
        .await?;

    let mut scenes = Vec::new();
    for row in scene_rows {
        let layers = fetch_layers(&state.db, &row.id).await?;
        scenes.push(Scene {
            id: row.id.parse().unwrap_or_default(),
            name: row.name,
            layers,
        });
    }

    Ok(Json(scenes))
}

/// Create a scene.
#[utoipa::path(
    post,
    path = "/api/v1/scenes",
    tag = "scenes",
    request_body = CreateScene,
    responses((status = 201, description = "Created scene", body = muxshed_common::Scene))
)]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateScene>,
) -> Result<(StatusCode, Json<Scene>), ApiError> {
    let id = Uuid::new_v4();

    sqlx::query("INSERT INTO scenes (id, name) VALUES (?, ?)")
        .bind(id.to_string())
        .bind(&body.name)
        .execute(&state.db)
        .await?;

    let mut layers = Vec::new();
    if let Some(create_layers) = body.layers {
        for cl in create_layers {
            let layer = insert_layer(&state.db, &id.to_string(), cl).await?;
            layers.push(layer);
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(Scene {
            id,
            name: body.name,
            layers,
        }),
    ))
}

/// Get a single scene by id.
#[utoipa::path(
    get,
    path = "/api/v1/scenes/{id}",
    tag = "scenes",
    params(("id" = String, Path, description = "Scene id")),
    responses(
        (status = 200, description = "Scene", body = muxshed_common::Scene),
        (status = 404, description = "Not found")
    )
)]
pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Scene>, ApiError> {
    let row = sqlx::query_as::<_, SceneRow>("SELECT id, name FROM scenes WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("scene {}", id)))?;

    let layers = fetch_layers(&state.db, &row.id).await?;

    Ok(Json(Scene {
        id: row.id.parse().unwrap_or_default(),
        name: row.name,
        layers,
    }))
}

/// Update a scene.
#[utoipa::path(
    put,
    path = "/api/v1/scenes/{id}",
    tag = "scenes",
    params(("id" = String, Path, description = "Scene id")),
    request_body = UpdateScene,
    responses(
        (status = 200, description = "Updated scene", body = muxshed_common::Scene),
        (status = 404, description = "Not found")
    )
)]
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateScene>,
) -> Result<Json<Scene>, ApiError> {
    let existing = sqlx::query_as::<_, SceneRow>("SELECT id, name FROM scenes WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("scene {}", id)))?;

    let name = body.name.unwrap_or(existing.name);

    sqlx::query("UPDATE scenes SET name = ? WHERE id = ?")
        .bind(&name)
        .bind(&id)
        .execute(&state.db)
        .await?;

    let layers = fetch_layers(&state.db, &id).await?;

    Ok(Json(Scene {
        id: id.parse().unwrap_or_default(),
        name,
        layers,
    }))
}

/// Delete a scene.
#[utoipa::path(
    delete,
    path = "/api/v1/scenes/{id}",
    tag = "scenes",
    params(("id" = String, Path, description = "Scene id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found")
    )
)]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM scenes WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("scene {}", id)).into());
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Activate a scene (make it the live program).
#[utoipa::path(
    post,
    path = "/api/v1/scenes/{id}/activate",
    tag = "scenes",
    params(("id" = String, Path, description = "Scene id")),
    responses(
        (status = 200, description = "Activated"),
        (status = 404, description = "Not found")
    )
)]
pub async fn activate(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _scene = sqlx::query_as::<_, SceneRow>("SELECT id, name FROM scenes WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("scene {}", id)))?;

    let scene_uuid: Uuid = id.parse().map_err(|_| MuxshedError::BadRequest("invalid uuid".to_string()))?;

    // Composite the scene's layers and route the result to program.
    crate::scene_compositor::stop_all_compositors(&state).await;
    crate::scene_compositor::start_scene_compositor(state.clone(), scene_uuid)
        .await
        .map_err(|e| MuxshedError::BadRequest(format!("cannot activate scene: {}", e)))?;
    let _ = state.program_intent.send(Some(scene_uuid));
    state.pipeline.activate_scene(&scene_uuid).await?;

    let _ = state.ws_tx.send(WsEvent::SceneChanged {
        scene_id: scene_uuid,
        method: "cut".to_string(),
    });

    Ok(StatusCode::OK)
}

/// Restart the compositor if this scene is currently the program (so live edits
/// to its layers take effect immediately).
async fn refresh_if_active(state: &Arc<AppState>, scene_id: &str) {
    if let Ok(uuid) = scene_id.parse::<Uuid>() {
        if *state.program_intent.borrow() == Some(uuid) {
            let _ = crate::scene_compositor::start_scene_compositor(state.clone(), uuid).await;
        }
    }
}

/// Add a layer to a scene.
#[utoipa::path(
    post,
    path = "/api/v1/scenes/{id}/layers",
    tag = "scenes",
    params(("id" = String, Path, description = "Scene id")),
    request_body = CreateLayer,
    responses(
        (status = 201, description = "Created layer", body = muxshed_common::Layer),
        (status = 404, description = "Not found")
    )
)]
pub async fn add_layer(
    State(state): State<Arc<AppState>>,
    Path(scene_id): Path<String>,
    Json(body): Json<CreateLayer>,
) -> Result<(StatusCode, Json<Layer>), ApiError> {
    let _scene = sqlx::query_as::<_, SceneRow>("SELECT id, name FROM scenes WHERE id = ?")
        .bind(&scene_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| MuxshedError::NotFound(format!("scene {}", scene_id)))?;

    let layer = insert_layer(&state.db, &scene_id, body).await?;
    refresh_if_active(&state, &scene_id).await;
    Ok((StatusCode::CREATED, Json(layer)))
}

/// Update a layer within a scene.
#[utoipa::path(
    put,
    path = "/api/v1/scenes/{scene_id}/layers/{layer_id}",
    tag = "scenes",
    params(
        ("scene_id" = String, Path, description = "Scene id"),
        ("layer_id" = String, Path, description = "Layer id")
    ),
    request_body = UpdateLayer,
    responses(
        (status = 200, description = "Updated layer", body = muxshed_common::Layer),
        (status = 404, description = "Not found")
    )
)]
pub async fn update_layer(
    State(state): State<Arc<AppState>>,
    Path((scene_id, layer_id)): Path<(String, String)>,
    Json(body): Json<UpdateLayer>,
) -> Result<Json<Layer>, ApiError> {
    let existing = sqlx::query_as::<_, LayerRow>(
        "SELECT id, scene_id, source_id, x, y, width, height, z_index, opacity, fit FROM scene_layers WHERE id = ? AND scene_id = ?",
    )
    .bind(&layer_id)
    .bind(&scene_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MuxshedError::NotFound(format!("layer {}", layer_id)))?;

    let source_id = body.source_id.map(|s| s.to_string()).unwrap_or(existing.source_id);
    let x = body.x.unwrap_or(existing.x);
    let y = body.y.unwrap_or(existing.y);
    let width = body.width.map(|w| w as i32).unwrap_or(existing.width);
    let height = body.height.map(|h| h as i32).unwrap_or(existing.height);
    let z_index = body.z_index.map(|z| z as i32).unwrap_or(existing.z_index);
    let opacity = body.opacity.unwrap_or(existing.opacity);
    let fit = body.fit.unwrap_or_else(|| LayerFit::from_db(&existing.fit));

    sqlx::query(
        "UPDATE scene_layers SET source_id = ?, x = ?, y = ?, width = ?, height = ?, z_index = ?, opacity = ?, fit = ? WHERE id = ?",
    )
    .bind(&source_id)
    .bind(x)
    .bind(y)
    .bind(width)
    .bind(height)
    .bind(z_index)
    .bind(opacity)
    .bind(fit.as_str())
    .bind(&layer_id)
    .execute(&state.db)
    .await?;

    refresh_if_active(&state, &scene_id).await;
    Ok(Json(Layer {
        id: layer_id.parse().unwrap_or_default(),
        source_id: source_id.parse().unwrap_or_default(),
        position: Position { x, y },
        size: Size {
            width: width as u32,
            height: height as u32,
        },
        z_index: z_index as u32,
        opacity,
        fit,
    }))
}

/// Delete a layer from a scene.
#[utoipa::path(
    delete,
    path = "/api/v1/scenes/{scene_id}/layers/{layer_id}",
    tag = "scenes",
    params(
        ("scene_id" = String, Path, description = "Scene id"),
        ("layer_id" = String, Path, description = "Layer id")
    ),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found")
    )
)]
pub async fn delete_layer(
    State(state): State<Arc<AppState>>,
    Path((scene_id, layer_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query("DELETE FROM scene_layers WHERE id = ? AND scene_id = ?")
        .bind(&layer_id)
        .bind(&scene_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MuxshedError::NotFound(format!("layer {}", layer_id)).into());
    }

    refresh_if_active(&state, &scene_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn insert_layer(
    db: &sqlx::SqlitePool,
    scene_id: &str,
    cl: CreateLayer,
) -> Result<Layer, ApiError> {
    let layer_id = Uuid::new_v4();
    let x = cl.x.unwrap_or(0);
    let y = cl.y.unwrap_or(0);
    let width = cl.width.unwrap_or(1920);
    let height = cl.height.unwrap_or(1080);
    let z_index = cl.z_index.unwrap_or(0);
    let opacity = cl.opacity.unwrap_or(1.0);
    let fit = cl.fit.unwrap_or_default();

    sqlx::query(
        "INSERT INTO scene_layers (id, scene_id, source_id, x, y, width, height, z_index, opacity, fit) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(layer_id.to_string())
    .bind(scene_id)
    .bind(cl.source_id.to_string())
    .bind(x)
    .bind(y)
    .bind(width as i32)
    .bind(height as i32)
    .bind(z_index as i32)
    .bind(opacity)
    .bind(fit.as_str())
    .execute(db)
    .await?;

    Ok(Layer {
        id: layer_id,
        source_id: cl.source_id,
        position: Position { x, y },
        size: Size { width, height },
        z_index,
        opacity,
        fit,
    })
}

async fn fetch_layers(db: &sqlx::SqlitePool, scene_id: &str) -> Result<Vec<Layer>, ApiError> {
    let rows = sqlx::query_as::<_, LayerRow>(
        "SELECT id, scene_id, source_id, x, y, width, height, z_index, opacity, fit FROM scene_layers WHERE scene_id = ? ORDER BY z_index",
    )
    .bind(scene_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_layer()).collect())
}

#[derive(sqlx::FromRow)]
struct SceneRow {
    id: String,
    name: String,
}

#[derive(sqlx::FromRow)]
struct LayerRow {
    id: String,
    #[allow(dead_code)]
    scene_id: String,
    source_id: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    z_index: i32,
    opacity: f32,
    fit: String,
}

impl LayerRow {
    fn into_layer(self) -> Layer {
        Layer {
            id: self.id.parse().unwrap_or_default(),
            source_id: self.source_id.parse().unwrap_or_default(),
            position: Position {
                x: self.x,
                y: self.y,
            },
            size: Size {
                width: self.width as u32,
                height: self.height as u32,
            },
            z_index: self.z_index as u32,
            opacity: self.opacity,
            fit: LayerFit::from_db(&self.fit),
        }
    }
}
