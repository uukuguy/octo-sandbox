//! Integration tests for the multi-session REST API endpoints (Phase AJ).
//!
//! Tests the session lifecycle REST endpoints:
//! - POST /api/v1/sessions/start
//! - GET  /api/v1/sessions/active
//! - GET  /api/v1/sessions/{id}/status
//! - DELETE /api/v1/sessions/{id}/stop
//!
//! Uses `TestApp` (tower::ServiceExt::oneshot) — no real port binding or
//! WebSocket connections.

mod common;

use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn test_start_session_endpoint() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": "test-s1", "agent_id": null}),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "expected 201 CREATED, got {status}");
    assert_eq!(
        body["session_id"], "test-s1",
        "response session_id should match request"
    );
    assert_eq!(
        body["status"], "active",
        "newly started session should be active"
    );
}

#[tokio::test]
async fn test_list_active_sessions() {
    let app = common::TestApp::new().await;

    // TestApp::new() already starts a primary session, so baseline count is 1.
    // Start two additional sessions.
    let (s1_status, _) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": "list-s1", "agent_id": null}),
        )
        .await;
    assert_eq!(s1_status, StatusCode::CREATED);

    let (s2_status, _) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": "list-s2", "agent_id": null}),
        )
        .await;
    assert_eq!(s2_status, StatusCode::CREATED);

    // List active sessions
    let (status, body) = app.get("/api/v1/sessions/active").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body["count"].as_u64().unwrap_or(0) >= 2,
        "expected at least 2 active sessions (primary + 2 new), got count={}",
        body["count"]
    );
    assert_eq!(
        body["max"].as_u64().unwrap_or(0),
        64,
        "max concurrent sessions should be 64"
    );
    assert!(
        body["sessions"].is_array(),
        "sessions should be an array"
    );
    assert!(
        !body["sessions"].as_array().unwrap().is_empty(),
        "sessions array should not be empty"
    );
}

#[tokio::test]
async fn test_session_status_endpoint() {
    let app = common::TestApp::new().await;

    // Start a session
    let (start_status, _) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": "test-status-1", "agent_id": null}),
        )
        .await;
    assert_eq!(start_status, StatusCode::CREATED);

    // Check status of existing session — should be active
    let (status, body) = app.get("/api/v1/sessions/test-status-1/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["session_id"], "test-status-1",
        "session_id in response should match"
    );
    assert_eq!(
        body["active"], true,
        "started session should be active"
    );

    // Check a non-existent session — should report active: false
    let (status2, body2) = app.get("/api/v1/sessions/nonexistent-xyz/status").await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(
        body2["session_id"], "nonexistent-xyz",
        "session_id should echo back the requested id"
    );
    assert_eq!(
        body2["active"], false,
        "non-existent session should not be active"
    );
}

#[tokio::test]
async fn test_stop_session_endpoint() {
    let app = common::TestApp::new().await;

    // Start a session
    let (start_status, _) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": "test-stop-1", "agent_id": null}),
        )
        .await;
    assert_eq!(start_status, StatusCode::CREATED);

    // Stop the session
    let (stop_status, stop_body) = app.delete("/api/v1/sessions/test-stop-1/stop").await;
    assert_eq!(stop_status, StatusCode::OK, "stop should return 200");
    assert_eq!(
        stop_body["status"], "stopped",
        "stop response should contain status=stopped"
    );

    // Verify it is no longer active
    let (_, status_body) = app.get("/api/v1/sessions/test-stop-1/status").await;
    assert_eq!(
        status_body["active"], false,
        "stopped session should no longer be active"
    );
}

#[tokio::test]
async fn test_session_lifecycle_full() {
    let app = common::TestApp::new().await;

    let sid = "lifecycle-full-1";

    // 1. Start session
    let (start_status, start_body) = app
        .post_json(
            "/api/v1/sessions/start",
            json!({"session_id": sid, "agent_id": null}),
        )
        .await;
    assert_eq!(start_status, StatusCode::CREATED);
    assert_eq!(start_body["session_id"], sid);
    assert_eq!(start_body["status"], "active");

    // 2. List — verify the session is present
    let (_, list_body) = app.get("/api/v1/sessions/active").await;
    let sessions = list_body["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert!(
        sessions.iter().any(|s| s.as_str() == Some(sid)),
        "active sessions list should contain '{sid}'"
    );
    let count_after_start = list_body["count"].as_u64().unwrap();

    // 3. Status — should be active
    let (_, status_body) = app.get(&format!("/api/v1/sessions/{sid}/status")).await;
    assert_eq!(status_body["active"], true);

    // 4. Stop
    let (stop_status, stop_body) = app.delete(&format!("/api/v1/sessions/{sid}/stop")).await;
    assert_eq!(stop_status, StatusCode::OK);
    assert_eq!(stop_body["status"], "stopped");

    // 5. Status — should no longer be active
    let (_, status_body2) = app.get(&format!("/api/v1/sessions/{sid}/status")).await;
    assert_eq!(
        status_body2["active"], false,
        "session should be inactive after stop"
    );

    // 6. List — verify count decreased or session absent
    let (_, list_body2) = app.get("/api/v1/sessions/active").await;
    let count_after_stop = list_body2["count"].as_u64().unwrap();
    assert!(
        count_after_stop < count_after_start,
        "active count should decrease after stopping a session (before={count_after_start}, after={count_after_stop})"
    );
    let sessions2 = list_body2["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert!(
        !sessions2.iter().any(|s| s.as_str() == Some(sid)),
        "stopped session '{sid}' should not appear in active sessions list"
    );
}
