// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use muxshed_api::auth::{generate_api_key, hash_key};
use muxshed_api::egress::EgressManager;
use muxshed_api::routes::build_router;
use muxshed_api::state::AppState;
use muxshed_common::{MuxshedConfig, WsEvent};
use muxshed_processor::StubPipelineController;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower::ServiceExt;

async fn setup() -> (axum::Router<()>, String, Arc<AppState>) {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();

    let (ws_tx, _) = broadcast::channel::<WsEvent>(64);
    let (program_tx, _) = broadcast::channel::<bytes::Bytes>(64);
    let (program_source_tx, _) = tokio::sync::watch::channel::<Option<uuid::Uuid>>(None);
    let (audio_routing_tx, _) = tokio::sync::watch::channel(muxshed_api::state::AudioRouting::default());
    let pipeline = Arc::new(StubPipelineController::new(ws_tx.clone()));

    let config = MuxshedConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        rtmp_port: 1935,
        db_path: PathBuf::from(":memory:"),
        data_dir: PathBuf::from("/tmp/muxshed-test"),
        web_dir: None,
        log_level: "error".to_string(),
    };

    let key = generate_api_key();
    let key_hash = hash_key(&key);
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO api_keys (id, name, key_hash, scopes) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind("test")
        .bind(&key_hash)
        .bind("[\"read\",\"control\",\"admin\"]")
        .execute(&db)
        .await
        .unwrap();

    let state = Arc::new(AppState {
        pipeline,
        config: Arc::new(RwLock::new(config)),
        db,
        egress: EgressManager::new(ws_tx.clone()),
        ws_tx,
        source_states: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        media_relays: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        sequence_headers: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        source_media_info: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        program_tx,
        program_source: program_source_tx,
        preview_source: tokio::sync::RwLock::new(None),
        audio_routing: audio_routing_tx,
    });

    (build_router(state.clone(), None), key, state)
}

fn json_request(method: &str, uri: &str, key: &str, body: Option<&str>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("X-API-Key", key);

    match body {
        Some(b) => builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn response_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

// --- Auth tests ---

#[tokio::test]
async fn test_unauthorized_without_key() {
    let (app, _key, _state2) = setup().await;
    let req = Request::builder()
        .uri("/api/v1/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unauthorized_with_bad_key() {
    let (app, _key, _state2) = setup().await;
    let req = json_request("GET", "/api/v1/status", "mxs_invalid_key_here", None);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_authorized_with_valid_key() {
    let (app, key, _state) = setup().await;
    let req = json_request("GET", "/api/v1/status", &key, None);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- Status tests ---

#[tokio::test]
async fn test_status_returns_idle() {
    let (app, key, _state) = setup().await;
    let req = json_request("GET", "/api/v1/status", &key, None);
    let resp = app.oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["pipeline"]["state"], "idle");
}

// --- Source CRUD tests ---

#[tokio::test]
async fn test_source_crud() {
    let (app, key, _state) = setup().await;

    // Create
    let req = json_request(
        "POST",
        "/api/v1/sources",
        &key,
        Some(r#"{"name":"OBS","kind":{"type":"rtmp","stream_key":""}}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "OBS");
    assert!(!body["kind"]["stream_key"].as_str().unwrap().is_empty());

    // List
    let req = json_request("GET", "/api/v1/sources", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Get one
    let req = json_request("GET", &format!("/api/v1/sources/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Update
    let req = json_request(
        "PUT",
        &format!("/api/v1/sources/{}", id),
        &key,
        Some(r#"{"name":"OBS Updated"}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["name"], "OBS Updated");

    // Delete
    let req = json_request("DELETE", &format!("/api/v1/sources/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify deleted
    let req = json_request("GET", &format!("/api/v1/sources/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// --- Destination CRUD tests ---

#[tokio::test]
async fn test_destination_crud() {
    let (app, key, _state) = setup().await;

    let req = json_request(
        "POST",
        "/api/v1/destinations",
        &key,
        Some(r#"{"name":"YouTube","kind":{"type":"rtmp","url":"rtmp://a.rtmp.youtube.com/live2","stream_key":"test123"}}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["enabled"], true);

    // Disable
    let req = json_request("POST", &format!("/api/v1/destinations/{}/disable", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Enable
    let req = json_request("POST", &format!("/api/v1/destinations/{}/enable", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Delete
    let req = json_request("DELETE", &format!("/api/v1/destinations/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// --- Stream lifecycle tests ---

#[tokio::test]
async fn test_stream_start_requires_destinations() {
    let (app, key, _state) = setup().await;

    let req = json_request("POST", "/api/v1/stream/start", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_stream_lifecycle() {
    let (app, key, state) = setup().await;

    // Create destination
    let req = json_request(
        "POST",
        "/api/v1/destinations",
        &key,
        Some(r#"{"name":"Test","kind":{"type":"rtmp","url":"rtmp://localhost","stream_key":"key"}}"#),
    );
    app.clone().oneshot(req).await.unwrap();

    // Create a source and simulate it being live
    let req = json_request(
        "POST",
        "/api/v1/sources",
        &key,
        Some(r#"{"name":"OBS","kind":{"type":"rtmp","stream_key":"testkey"}}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    let source_id: uuid::Uuid = body["id"].as_str().unwrap().parse().unwrap();

    // Simulate live source with media relay
    {
        let mut states = state.source_states.write().await;
        states.insert(source_id, muxshed_common::SourceState::Live);
    }
    let _ = state.get_or_create_media_relay(source_id).await;

    // Start — egress may fail (no real RTMP server) but pipeline state transitions
    let req = json_request("POST", "/api/v1/stream/start", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    if resp.status() != StatusCode::OK {
        let body = response_json(resp).await;
        panic!("stream start failed: {:?}", body);
    }

    // Wait for stub to transition to live
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Status should be live
    let req = json_request("GET", "/api/v1/status", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["pipeline"]["state"], "live");

    // Stop
    let req = json_request("POST", "/api/v1/stream/stop", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Status should be idle
    let req = json_request("GET", "/api/v1/status", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["pipeline"]["state"], "idle");
}

// --- Scene CRUD tests ---

#[tokio::test]
async fn test_scene_crud() {
    let (app, key, _state) = setup().await;

    // Create scene
    let req = json_request(
        "POST",
        "/api/v1/scenes",
        &key,
        Some(r#"{"name":"Main"}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let scene_id = body["id"].as_str().unwrap().to_string();

    // Create source for layer
    let req = json_request(
        "POST",
        "/api/v1/sources",
        &key,
        Some(r#"{"name":"Cam1","kind":{"type":"rtmp","stream_key":""}}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let source_body = response_json(resp).await;
    let source_id = source_body["id"].as_str().unwrap();

    // Add layer
    let req = json_request(
        "POST",
        &format!("/api/v1/scenes/{}/layers", scene_id),
        &key,
        Some(&format!(r#"{{"source_id":"{}","width":1920,"height":1080}}"#, source_id)),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let layer_body = response_json(resp).await;
    let layer_id = layer_body["id"].as_str().unwrap().to_string();

    // Get scene with layers
    let req = json_request("GET", &format!("/api/v1/scenes/{}", scene_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["layers"].as_array().unwrap().len(), 1);

    // Delete layer
    let req = json_request(
        "DELETE",
        &format!("/api/v1/scenes/{}/layers/{}", scene_id, layer_id),
        &key,
        None,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Delete scene
    let req = json_request("DELETE", &format!("/api/v1/scenes/{}", scene_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// --- Stinger CRUD tests ---

#[tokio::test]
async fn test_stinger_crud() {
    let (app, key, _state) = setup().await;

    let req = json_request(
        "POST",
        "/api/v1/stingers",
        &key,
        Some(r#"{"name":"Wipe","file_path":"/stingers/wipe.webm","opaque_ms":500,"clear_ms":1000,"end_ms":1500}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let id = body["id"].as_str().unwrap().to_string();

    // Update markers
    let req = json_request(
        "PUT",
        &format!("/api/v1/stingers/{}", id),
        &key,
        Some(r#"{"opaque_ms":600}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["opaque_ms"], 600);

    // Delete
    let req = json_request("DELETE", &format!("/api/v1/stingers/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// --- Delay tests ---

#[tokio::test]
async fn test_delay_config() {
    let (app, key, _state) = setup().await;

    // Get default
    let req = json_request("GET", "/api/v1/delay", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["enabled"], false);
    assert_eq!(body["duration_ms"], 7000);

    // Update
    let req = json_request(
        "PUT",
        "/api/v1/delay",
        &key,
        Some(r#"{"enabled":true,"duration_ms":10000}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["duration_ms"], 10000);

    // Bleep
    let req = json_request("POST", "/api/v1/delay/bleep", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- Overlay tests ---

#[tokio::test]
async fn test_overlay_crud() {
    let (app, key, _state) = setup().await;

    let req = json_request(
        "POST",
        "/api/v1/overlays",
        &key,
        Some(r#"{"name":"Logo","kind":{"type":"image","file_path":"/overlays/logo.png"}}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["visible"], false);

    // Show
    let req = json_request("POST", &format!("/api/v1/overlays/{}/show", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Hide
    let req = json_request("POST", &format!("/api/v1/overlays/{}/hide", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Delete
    let req = json_request("DELETE", &format!("/api/v1/overlays/{}", id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// --- Guest tests ---

#[tokio::test]
async fn test_guest_invite() {
    let (app, key, _state) = setup().await;

    let req = json_request(
        "POST",
        "/api/v1/guests/invite",
        &key,
        Some(r#"{"name":"John"}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    assert!(!body["token"].as_str().unwrap().is_empty());
    assert!(body["url"].as_str().unwrap().contains("token="));

    // List
    let req = json_request("GET", "/api/v1/guests", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
}

// --- API Key tests ---

#[tokio::test]
async fn test_key_crud() {
    let (app, key, _state) = setup().await;

    // Create new key
    let req = json_request(
        "POST",
        "/api/v1/keys",
        &key,
        Some(r#"{"name":"StreamDeck","scopes":["read","control"]}"#),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    let new_key = body["key"].as_str().unwrap();
    assert!(new_key.starts_with("mxs_"));
    let new_id = body["id"].as_str().unwrap().to_string();

    // List (should have 2: test + new)
    let req = json_request("GET", "/api/v1/keys", &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 2);

    // Delete new key
    let req = json_request("DELETE", &format!("/api/v1/keys/{}", new_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

// --- Auth login tests ---

#[tokio::test]
async fn test_setup_and_login() {
    let (app, _key, _state2) = setup().await;

    // Setup status should show not needed (test setup already created a key)
    let req = Request::builder()
        .uri("/api/v1/setup/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = response_json(resp).await;
    // needs_setup checks users table, which is empty in test setup
    assert_eq!(body["needs_setup"], true);

    // Run setup init
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/setup/init")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"instance_name":"Test Studio"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_json(resp).await;
    assert_eq!(body["username"], "admin");
    assert!(!body["api_key"].as_str().unwrap().is_empty());

    // Login with default credentials
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"admin"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    let token = body["token"].as_str().unwrap();
    assert!(!token.is_empty());
    assert_eq!(body["username"], "admin");

    // Use session token to access protected endpoint
    let req = Request::builder()
        .uri("/api/v1/status")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Bad login
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"wrong"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Setup init should fail (already done)
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/setup/init")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// --- Not found tests ---

#[tokio::test]
async fn test_not_found_responses() {
    let (app, key, _state) = setup().await;
    let fake_id = "00000000-0000-0000-0000-000000000000";

    let req = json_request("GET", &format!("/api/v1/sources/{}", fake_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = json_request("DELETE", &format!("/api/v1/destinations/{}", fake_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = json_request("GET", &format!("/api/v1/scenes/{}", fake_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = json_request("GET", &format!("/api/v1/stingers/{}", fake_id), &key, None);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
