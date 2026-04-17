// Licensed under the Business Source License 1.1 — see LICENSE.

mod assets;
mod audio;
mod auth;
mod broadcast;
mod delay;
mod preview;
mod destinations;
mod guests;
mod keys;
pub mod output;
mod recording;
mod scenes;
mod setup;
mod sources;
mod status;
mod stingers;
mod stream;
mod switching;
mod ws;

use crate::auth::auth_middleware;
use crate::state::AppState;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use std::sync::Arc;
use axum::extract::DefaultBodyLimit;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

pub fn build_router(state: Arc<AppState>, web_dir: Option<std::path::PathBuf>) -> Router {
    let api = Router::new()
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
        // Delay / Bleep
        .route("/delay", get(delay::get_delay).put(delay::update_delay))
        .route("/delay/enable", post(delay::enable))
        .route("/delay/disable", post(delay::disable))
        .route("/delay/bleep", post(delay::bleep))
        // Output config and stats
        .route("/output/config", get(output::get_config).put(output::set_config))
        .route("/output/stats", get(output::get_stats))
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
        .route("/setup/init", post(setup::init));

    let mut router = Router::new().nest("/api/v1", api);

    if let Some(ref dir) = web_dir {
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
