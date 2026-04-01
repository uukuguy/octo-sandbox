//! E2E tests for Metering API endpoints (AO-T1)

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn metering_snapshot_returns_token_usage() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/metering/snapshot").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["input_tokens"].is_number());
    assert!(body["output_tokens"].is_number());
    assert!(body["total_tokens"].is_number());
    assert!(body["requests"].is_number());
    assert!(body["errors"].is_number());
    assert!(body["avg_tokens_per_request"].is_number());
    assert!(body["avg_duration_ms"].is_number());
}

#[tokio::test]
async fn metering_summary_returns_model_breakdown() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/metering/summary").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["models"].is_array());
    assert!(body["total_estimated_cost_usd"].is_number());
}

#[tokio::test]
async fn metering_by_session_returns_list() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/metering/by-session").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn metering_reset_returns_no_content() {
    let app = common::TestApp::new().await;
    let (status, _body) = app.post_json("/api/v1/metering/reset", serde_json::json!({})).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}
