// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

pub mod admin;
mod assets;
mod audio;
mod auth;
mod failover;
mod broadcast;
mod channel;
mod preview;
mod destinations;
mod guests;
mod keys;
pub mod output;
mod recording;
mod scenes;
mod schedules;
mod setup;
mod sources;
mod status;
mod stingers;
pub mod stream;
mod switching;
pub mod webrtc_config;
mod ws;

use crate::auth::auth_middleware;
use crate::state::AppState;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use std::sync::Arc;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

/// Root notice for headless instances (no web UI). Points at the API and docs.
async fn headless_root() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "muxshed",
        "version": env!("CARGO_PKG_VERSION"),
        "headless": true,
        "api": "/api/v1",
        "docs": "/api/v1/docs"
    }))
}

/// 404 for unmatched non-API routes in headless, with a pointer to the API.
async fn headless_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": { "code": "NOT_FOUND", "message": "no web UI in headless mode; use /api/v1" }
        })),
    )
}

pub fn build_router(
    state: Arc<AppState>,
    web_dir: Option<std::path::PathBuf>,
    headless: bool,
) -> Router {
    let mut api = Router::new()
        // Sources
        .route("/sources", get(sources::list).post(sources::create))
        .route("/sources/from-asset", post(sources::create_from_asset))
        .route(
            "/sources/{id}",
            get(sources::get_one)
                .put(sources::update)
                .delete(sources::delete),
        )
        // Scenes
        .route("/scenes", get(scenes::list).post(scenes::create))
        .route(
            "/scenes/{id}",
            get(scenes::get_one)
                .put(scenes::update)
                .delete(scenes::delete),
        )
        .route("/scenes/{id}/activate", post(scenes::activate))
        .route("/scenes/{id}/layers", post(scenes::add_layer))
        .route(
            "/scenes/{scene_id}/layers/{layer_id}",
            put(scenes::update_layer).delete(scenes::delete_layer),
        )
        // Assets
        .route("/assets", get(assets::list_assets))
        .route("/assets/{id}", get(assets::get_asset).put(assets::update_asset).delete(assets::delete_asset))
        .route("/folders", get(assets::list_folders).post(assets::create_folder))
        .route("/folders/{id}", put(assets::update_folder).delete(assets::delete_folder))
        // Library (stingers/transitions — legacy)
        .route("/library", get(stingers::list).post(stingers::create))
        .route(
            "/library/{id}",
            get(stingers::get_one)
                .put(stingers::update)
                .delete(stingers::delete),
        )
        // Legacy stinger routes
        .route("/stingers", get(stingers::list).post(stingers::create))
        .route(
            "/stingers/{id}",
            get(stingers::get_one)
                .put(stingers::update)
                .delete(stingers::delete),
        )
        // Transitions
        .route("/transition/stinger", post(stingers::trigger))
        // Destinations
        .route(
            "/destinations",
            get(destinations::list).post(destinations::create),
        )
        .route(
            "/destinations/{id}",
            put(destinations::update).delete(destinations::delete),
        )
        .route("/destinations/{id}/enable", post(destinations::enable))
        .route("/destinations/{id}/disable", post(destinations::disable))
        // Output config and stats
        .route("/output/config", get(output::get_config).put(output::set_config))
        .route("/failover", get(failover::get_config).put(failover::set_config))
        // Schedules
        .route("/schedules", get(schedules::list).post(schedules::create))
        .route("/schedules/{id}", put(schedules::update).delete(schedules::delete))
        .route("/schedules/{id}/run", post(schedules::run_now))
        .route("/schedules/stop", post(schedules::stop))
        .route("/settings/timezone", get(schedules::get_timezone).put(schedules::set_timezone))
        .route(
            "/webrtc/config",
            get(webrtc_config::get_config).put(webrtc_config::set_config),
        )
        .route("/output/stats", get(output::get_stats))
        // Public channel (HLS broadcast) config
        .route("/channel", get(channel::get_channel).put(channel::update_channel))
        .route("/channel/regenerate-token", post(channel::regenerate_token))
        .route(
            "/channel/logo",
            post(channel::upload_logo).delete(channel::delete_logo),
        )
        .route("/channel/password", put(channel::set_password))
        // Broadcast config
        .route("/broadcast/config", get(broadcast::get_config).put(broadcast::set_config))
        // Audio routing
        .route("/audio/routing", get(audio::get_routing).put(audio::set_routing))
        .route("/audio/source", post(audio::set_audio_source))
        .route("/audio/follows-video", post(audio::toggle_follows_video))
        .route("/audio/mute/{source_id}", post(audio::mute_source))
        .route("/audio/unmute/{source_id}", post(audio::unmute_source))
        // Source switching
        .route("/program", get(switching::get_program))
        .route("/preview/{source_id}", post(switching::set_preview))
        .route("/cut/{source_id}", post(switching::cut))
        .route("/auto", post(switching::auto))
        // Stream control
        .route("/stream/start", post(stream::start))
        .route("/stream/stop", post(stream::stop))
        // Recording
        .route("/record/start", post(recording::start))
        .route("/record/stop", post(recording::stop))
        .route("/record/status", get(recording::status))
        // Guests
        .route("/guests/invite", post(guests::invite))
        .route("/guests", get(guests::list))
        .route("/guests/{id}", delete(guests::delete))
        // Status and keys
        .route("/status", get(status::get_status))
        .route("/keys", get(keys::list).post(keys::create))
        .route("/keys/{id}", delete(keys::delete))
        // Auth-protected user routes
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/change-password", post(auth::change_password))
        // User management
        .route("/users", get(auth::list_users).post(auth::create_user))
        .route("/users/{id}", put(auth::update_user).delete(auth::delete_user))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        // Unauthenticated routes (handle auth internally)
        .route("/ws", get(ws::handler))
        .route("/sources/{id}/preview", get(preview::handler))
        .route("/assets/upload", post(assets::upload_asset).layer(DefaultBodyLimit::max(500 * 1024 * 1024)))
        .route("/assets/{id}/file", get(assets::serve_file))
        .route("/assets/{id}/thumbnail", get(assets::serve_thumbnail))
        .route("/library/upload", post(stingers::upload).layer(DefaultBodyLimit::max(500 * 1024 * 1024)))
        .route("/auth/login", post(auth::login))
        .route("/setup/status", get(setup::status))
        .route("/setup/init", post(setup::init))
        // Public channel (unlisted token URL — no auth)
        .route("/public/channel/{token}", get(channel::public_info))
        .route("/public/channel/{token}/unlock", post(channel::unlock))
        .route("/public/channel/{token}/index.m3u8", get(channel::serve_playlist))
        .route("/public/channel/{token}/logo", get(channel::serve_logo))
        .route("/public/channel/{token}/{segment}", get(channel::serve_segment))
        // Public guest join (token-scoped — no auth)
        .route("/guest/{token}", get(guests::public_info))
        .route("/guest/{token}/whip", post(guests::whip));

    // Privileged management group, only mounted when a management token is set.
    // Gated by management_auth (X-Management-Token), separate from tenant keys.
    if state.system_token.is_some() {
        let admin_router = Router::new()
            .route("/health", get(admin::health))
            .route("/stats", get(admin::stats))
            .route("/connectivity", get(admin::connectivity))
            .route("/restart", post(admin::restart))
            .route("/config", get(admin::get_config).put(admin::put_config))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::management_auth,
            ));
        api = api.nest("/admin", admin_router);
        tracing::info!("management endpoints enabled at /api/v1/admin");
    }

    let mut router = Router::new().nest("/api/v1", api);

    if headless {
        // No web UI: a JSON notice at the root, JSON 404 for everything else.
        router = router.route("/", get(headless_root)).fallback(headless_not_found);
        tracing::info!("headless mode — web UI not served");
    } else if let Some(ref dir) = web_dir {
        if dir.exists() {
            let index = dir.join("index.html");
            router = router.fallback_service(
                ServeDir::new(dir).fallback(ServeFile::new(index)),
            );
            tracing::info!("serving frontend from {}", dir.display());
        }
    }

    router
        .layer(CorsLayer::permissive())
        .with_state(state)
}
