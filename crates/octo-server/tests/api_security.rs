//! Security middleware integration tests (Phase AK-T7).
//!
//! Tests:
//!   - Security response headers on all endpoints
//!   - Liveness probe endpoint
//!   - API v1 routing (old /api/ paths return 404)
//!   - Request body size limit enforcement

mod common;

use axum::http::StatusCode;

// ── Security Headers ──────────────────────────────────────────────────

#[tokio::test]
async fn security_headers_present_on_health() {
    let app = common::TestApp::new().await;
    let (status, _body, headers) = app.get_with_headers("/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("x-content-type-options").map(|v| v.to_str().unwrap()),
        Some("nosniff")
    );
    assert_eq!(
        headers.get("x-frame-options").map(|v| v.to_str().unwrap()),
        Some("DENY")
    );
    assert_eq!(
        headers.get("referrer-policy").map(|v| v.to_str().unwrap()),
        Some("strict-origin-when-cross-origin")
    );
    // HSTS should NOT be present (TLS disabled in test)
    assert!(headers.get("strict-transport-security").is_none());
}

#[tokio::test]
async fn security_headers_present_on_api_v1() {
    let app = common::TestApp::new().await;
    let (status, _body, headers) = app.get_with_headers("/api/v1/config").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("x-content-type-options").map(|v| v.to_str().unwrap()),
        Some("nosniff")
    );
    assert_eq!(
        headers.get("x-frame-options").map(|v| v.to_str().unwrap()),
        Some("DENY")
    );
}

// ── Liveness Probe ────────────────────────────────────────────────────

#[tokio::test]
async fn liveness_probe_returns_ok() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/health/live").await;

    assert_eq!(status, StatusCode::OK);
    // Liveness returns plain text "ok", which won't parse as JSON
    // so body will be Null from our JSON parser
    assert!(body.is_null() || body.as_str() == Some("ok"));
}

// ── API v1 Routing ────────────────────────────────────────────────────

#[tokio::test]
async fn api_v1_config_reachable() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/config").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["provider"].is_string());
}

#[tokio::test]
async fn old_api_config_returns_404() {
    let app = common::TestApp::new().await;
    // Old path without /v1/ should no longer work
    let (status, _body) = app.get("/api/config").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_v1_sessions_reachable() {
    let app = common::TestApp::new().await;
    let (status, _body) = app.get("/api/v1/sessions").await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn api_v1_tools_reachable() {
    let app = common::TestApp::new().await;
    let (status, _body) = app.get("/api/v1/tools").await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn api_v1_agents_reachable() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/agents").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "agents endpoint should return an array");
}

// ── Request Body Size Limit ───────────────────────────────────────────

#[tokio::test]
async fn oversized_request_body_rejected() {
    let app = common::TestApp::new().await;
    // Default limit is 10MB. Send 11MB of data.
    let large_body = "x".repeat(11 * 1024 * 1024);
    let payload = serde_json::json!({ "data": large_body });

    let (status, _body) = app.post_json("/api/v1/memories", payload).await;

    // Should be rejected with 413 Payload Too Large
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
