//! E2E tests for Sandbox Management API endpoints (AO-T7)

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn sandbox_status_returns_host_mode() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/sandbox/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mode"].as_str().unwrap(), "host");
    assert_eq!(body["sandbox_available"].as_bool().unwrap(), false);
    assert_eq!(body["active_count"].as_u64().unwrap(), 0);
    assert!(body["active_sessions"].as_array().unwrap().is_empty());
    assert!(body["config"].is_null());
}

#[tokio::test]
async fn sandbox_sessions_returns_empty_in_host_mode() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/sandbox/sessions").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn sandbox_release_unavailable_in_host_mode() {
    let app = common::TestApp::new().await;
    let (status, body) =
        app.post_json("/api/v1/sandbox/test-session/release", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"].as_str().unwrap().contains("host mode"));
}

#[tokio::test]
async fn sandbox_cleanup_unavailable_in_host_mode() {
    let app = common::TestApp::new().await;
    let (status, body) =
        app.post_json("/api/v1/sandbox/cleanup", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["error"].as_str().unwrap().contains("host mode"));
}
