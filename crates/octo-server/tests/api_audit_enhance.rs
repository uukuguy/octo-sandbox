mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn audit_stats_returns_structure() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get("/api/v1/audit/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("total").is_some());
    assert!(body.get("by_event_type").is_some());
    assert!(body.get("by_result").is_some());
}

#[tokio::test]
async fn audit_export_returns_array() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get("/api/v1/audit/export").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn audit_export_with_date_range() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .get("/api/v1/audit/export?since=2020-01-01T00:00:00Z&until=2099-12-31T23:59:59Z")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
}

#[tokio::test]
async fn audit_delete_with_before() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .delete("/api/v1/audit?before=2020-01-01T00:00:00Z")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("deleted_count").is_some());
    assert_eq!(body["deleted_count"], 0);
}
