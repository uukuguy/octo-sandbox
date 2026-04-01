mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn context_snapshot_returns_budget() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get("/api/v1/context/snapshot").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("total_budget").is_some());
    assert_eq!(body["total_budget"], 200_000);
    assert!(body.get("usage_pct").is_some());
    assert!(body.get("needs_pruning").is_some());
    assert!(!body["needs_pruning"].as_bool().unwrap());
    assert_eq!(body["degradation_level"], "none");
}

#[tokio::test]
async fn context_zones_returns_four_zones() {
    let app = common::TestApp::new().await;

    let (status, body) = app.get("/api/v1/context/zones").await;

    assert_eq!(status, StatusCode::OK);

    // Check all four zones exist
    for zone_key in &["zone_a", "zone_b", "zone_c", "zone_d"] {
        let zone = body.get(zone_key).unwrap_or_else(|| panic!("missing {}", zone_key));
        assert!(zone.get("name").is_some(), "{} missing name", zone_key);
        assert!(zone.get("tokens").is_some(), "{} missing tokens", zone_key);
        assert!(zone.get("description").is_some(), "{} missing description", zone_key);
    }

    assert_eq!(body["zone_a"]["name"], "System Prompt");
    assert_eq!(body["zone_c"]["name"], "Conversation History");
    assert_eq!(body["zone_d"]["name"], "Tool Definitions");
}
