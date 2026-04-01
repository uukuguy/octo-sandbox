//! E2E tests for Knowledge Graph API endpoints (AO-T2)

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn kg_stats_returns_counts() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/knowledge-graph/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["entity_count"].is_number());
    assert!(body["relation_count"].is_number());
    assert!(body["type_count"].is_number());
}

#[tokio::test]
async fn kg_create_and_get_entity() {
    let app = common::TestApp::new().await;

    // Create entity
    let (status, body) = app
        .post_json(
            "/api/v1/knowledge-graph/entities",
            serde_json::json!({
                "name": "TestEntity",
                "entity_type": "concept",
                "properties": {"key": "value"}
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let entity_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "TestEntity");
    assert_eq!(body["entity_type"], "concept");

    // Get entity
    let (status, body) = app
        .get(&format!(
            "/api/v1/knowledge-graph/entities/{}",
            entity_id
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "TestEntity");
}

#[tokio::test]
async fn kg_entity_not_found() {
    let app = common::TestApp::new().await;
    let (status, _) = app
        .get("/api/v1/knowledge-graph/entities/nonexistent")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn kg_create_relation_and_get_relations() {
    let app = common::TestApp::new().await;

    // Create two entities
    let (_, e1) = app
        .post_json(
            "/api/v1/knowledge-graph/entities",
            serde_json::json!({
                "name": "Entity1", "entity_type": "node"
            }),
        )
        .await;
    let (_, e2) = app
        .post_json(
            "/api/v1/knowledge-graph/entities",
            serde_json::json!({
                "name": "Entity2", "entity_type": "node"
            }),
        )
        .await;
    let id1 = e1["id"].as_str().unwrap().to_string();
    let id2 = e2["id"].as_str().unwrap().to_string();

    // Create relation
    let (status, rel) = app
        .post_json(
            "/api/v1/knowledge-graph/relations",
            serde_json::json!({
                "source_id": id1,
                "target_id": id2,
                "relation_type": "depends_on"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(rel["relation_type"], "depends_on");

    // Get relations for entity1
    let (status, body) = app
        .get(&format!(
            "/api/v1/knowledge-graph/entities/{}/relations",
            id1
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["outgoing"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn kg_delete_entity() {
    let app = common::TestApp::new().await;

    // Create entity
    let (_, e1) = app
        .post_json(
            "/api/v1/knowledge-graph/entities",
            serde_json::json!({
                "name": "A", "entity_type": "node"
            }),
        )
        .await;
    let id1 = e1["id"].as_str().unwrap().to_string();

    // Delete entity
    let (status, _) = app
        .delete(&format!(
            "/api/v1/knowledge-graph/entities/{}",
            id1
        ))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify deleted
    let (status, _) = app
        .get(&format!(
            "/api/v1/knowledge-graph/entities/{}",
            id1
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn kg_traverse_bfs() {
    let app = common::TestApp::new().await;

    // Create entities
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({
            "id": "root", "name": "Root", "entity_type": "node"
        }),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({
            "id": "child1", "name": "Child1", "entity_type": "node"
        }),
    )
    .await;

    // Create relation
    app.post_json(
        "/api/v1/knowledge-graph/relations",
        serde_json::json!({
            "source_id": "root", "target_id": "child1", "relation_type": "has_child"
        }),
    )
    .await;

    // Traverse
    let (status, body) = app
        .get("/api/v1/knowledge-graph/traverse?start=root&depth=2")
        .await;
    assert_eq!(status, StatusCode::OK);
    let nodes = body.as_array().unwrap();
    assert!(nodes.len() >= 2); // root + child1
    assert_eq!(nodes[0]["depth"], 0);
}

#[tokio::test]
async fn kg_search_entities() {
    let app = common::TestApp::new().await;

    // Create entities
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({
            "name": "FooBar", "entity_type": "concept"
        }),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({
            "name": "BazQux", "entity_type": "concept"
        }),
    )
    .await;

    // Search
    let (status, body) = app
        .get("/api/v1/knowledge-graph/entities?q=Foo")
        .await;
    assert_eq!(status, StatusCode::OK);
    let results = body.as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "FooBar");
}

#[tokio::test]
async fn kg_find_path() {
    let app = common::TestApp::new().await;

    // A -> B -> C
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({"id": "a", "name": "A", "entity_type": "node"}),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({"id": "b", "name": "B", "entity_type": "node"}),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/entities",
        serde_json::json!({"id": "c", "name": "C", "entity_type": "node"}),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/relations",
        serde_json::json!({"source_id": "a", "target_id": "b", "relation_type": "link"}),
    )
    .await;
    app.post_json(
        "/api/v1/knowledge-graph/relations",
        serde_json::json!({"source_id": "b", "target_id": "c", "relation_type": "link"}),
    )
    .await;

    let (status, body) = app
        .get("/api/v1/knowledge-graph/path?from=a&to=c")
        .await;
    assert_eq!(status, StatusCode::OK);
    let path = body["path"].as_array().unwrap();
    assert_eq!(path.len(), 3); // a, b, c
    assert_eq!(body["length"], 2);
}
