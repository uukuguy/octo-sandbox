//! Knowledge Graph API — entity-relation graph operations (AO-T2)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use octo_engine::memory::{Entity, GraphStats, Relation};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ── Request/Response types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub id: Option<String>,
    pub name: String,
    pub entity_type: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationRequest {
    pub id: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EntityResponse {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Entity> for EntityResponse {
    fn from(e: &Entity) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            entity_type: e.entity_type.clone(),
            properties: e.properties.clone(),
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

impl From<Entity> for EntityResponse {
    fn from(e: Entity) -> Self {
        Self {
            id: e.id,
            name: e.name,
            entity_type: e.entity_type,
            properties: e.properties,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RelationResponse {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub properties: serde_json::Value,
    pub created_at: i64,
}

impl From<&Relation> for RelationResponse {
    fn from(r: &Relation) -> Self {
        Self {
            id: r.id.clone(),
            source_id: r.source_id.clone(),
            target_id: r.target_id.clone(),
            relation_type: r.relation_type.clone(),
            properties: r.properties.clone(),
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TraversalNode {
    pub id: String,
    pub entity: EntityResponse,
    pub depth: usize,
}

#[derive(Debug, Deserialize)]
pub struct TraverseQuery {
    pub start: String,
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_depth() -> usize {
    3
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub from: String,
    pub to: String,
}

// ── Router ────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/knowledge-graph/entities",
            get(search_entities).post(create_entity),
        )
        .route(
            "/knowledge-graph/entities/{id}",
            get(get_entity).delete(delete_entity),
        )
        .route(
            "/knowledge-graph/entities/{id}/relations",
            get(get_entity_relations),
        )
        .route("/knowledge-graph/relations", post(create_relation))
        .route("/knowledge-graph/stats", get(stats))
        .route("/knowledge-graph/traverse", get(traverse))
        .route("/knowledge-graph/path", get(find_path))
}

// ── Handlers ──────────────────────────────────────────────────────

/// GET /api/v1/knowledge-graph/entities?q=xxx&limit=50
async fn search_entities(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Json<Vec<EntityResponse>> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    let q = query.q.as_deref().unwrap_or("");
    let results: Vec<EntityResponse> = kg
        .search(q)
        .into_iter()
        .take(query.limit)
        .map(EntityResponse::from)
        .collect();
    Json(results)
}

/// POST /api/v1/knowledge-graph/entities
async fn create_entity(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateEntityRequest>,
) -> (StatusCode, Json<EntityResponse>) {
    let now = chrono::Utc::now().timestamp();
    let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let entity = Entity {
        id,
        name: req.name,
        entity_type: req.entity_type,
        properties: if req.properties.is_null() {
            serde_json::json!({})
        } else {
            req.properties
        },
        created_at: now,
        updated_at: now,
    };

    let resp = EntityResponse::from(&entity);
    let mut kg = state.agent_supervisor.knowledge_graph().write().await;
    kg.add_entity(entity);

    (StatusCode::CREATED, Json(resp))
}

/// GET /api/v1/knowledge-graph/entities/:id
async fn get_entity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<EntityResponse>, StatusCode> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    kg.get_entity(&id)
        .map(|e| Json(EntityResponse::from(e)))
        .ok_or(StatusCode::NOT_FOUND)
}

/// DELETE /api/v1/knowledge-graph/entities/:id
async fn delete_entity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut kg = state.agent_supervisor.knowledge_graph().write().await;
    match kg.remove_entity(&id) {
        Some(_) => StatusCode::NO_CONTENT,
        None => StatusCode::NOT_FOUND,
    }
}

/// GET /api/v1/knowledge-graph/entities/:id/relations
async fn get_entity_relations(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    if kg.get_entity(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let outgoing: Vec<RelationResponse> = kg
        .get_outgoing(&id)
        .iter()
        .map(|r| RelationResponse::from(*r))
        .collect();
    let incoming: Vec<RelationResponse> = kg
        .get_incoming(&id)
        .iter()
        .map(|r| RelationResponse::from(*r))
        .collect();
    Ok(Json(serde_json::json!({
        "outgoing": outgoing,
        "incoming": incoming,
    })))
}

/// POST /api/v1/knowledge-graph/relations
async fn create_relation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRelationRequest>,
) -> Result<(StatusCode, Json<RelationResponse>), StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let relation = Relation {
        id,
        source_id: req.source_id,
        target_id: req.target_id,
        relation_type: req.relation_type,
        properties: if req.properties.is_null() {
            serde_json::json!({})
        } else {
            req.properties
        },
        created_at: now,
    };

    let resp = RelationResponse::from(&relation);
    let mut kg = state.agent_supervisor.knowledge_graph().write().await;
    if kg.add_relation(relation) {
        Ok((StatusCode::CREATED, Json(resp)))
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

/// GET /api/v1/knowledge-graph/stats
async fn stats(State(state): State<Arc<AppState>>) -> Json<GraphStats> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    Json(kg.stats())
}

/// GET /api/v1/knowledge-graph/traverse?start=X&depth=N
async fn traverse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TraverseQuery>,
) -> Result<Json<Vec<TraversalNode>>, StatusCode> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    if kg.get_entity(&query.start).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let results = kg.traverse_bfs(&query.start, query.depth);
    let nodes: Vec<TraversalNode> = results
        .into_iter()
        .map(|(id, entity, depth)| TraversalNode {
            id,
            entity: EntityResponse::from(entity),
            depth,
        })
        .collect();
    Ok(Json(nodes))
}

/// GET /api/v1/knowledge-graph/path?from=X&to=Y
async fn find_path(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PathQuery>,
) -> Json<serde_json::Value> {
    let kg = state.agent_supervisor.knowledge_graph().read().await;
    match kg.find_path(&query.from, &query.to) {
        Some(ref path) => Json(serde_json::json!({
            "path": path,
            "length": path.len() - 1,
        })),
        None => Json(serde_json::json!({
            "path": null,
            "length": null,
        })),
    }
}
