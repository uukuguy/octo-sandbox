use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use crate::models::{
    InvokeById, InvokePromote, InvokeSkillRead, InvokeSkillSearch, PromoteRequest, SearchQuery,
    SubmitDraftRequest,
};
use crate::store::SkillStore;

/// Build the Axum router with all skill registry routes.
pub fn router(store: Arc<SkillStore>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/skills/search", get(search_skills))
        .route("/skills/draft", post(submit_draft))
        .route("/skills/{id}/content", get(get_skill_content))
        .route("/skills/{id}/versions", get(list_versions))
        .route("/skills/{id}/promote/{version}", post(promote_skill))
        // MCP-style tool discovery + invocation (S3.T1)
        .route("/tools", get(list_tools))
        .route("/tools/skill_search/invoke", post(invoke_skill_search))
        .route("/tools/skill_read/invoke", post(invoke_skill_read))
        .route(
            "/tools/skill_list_versions/invoke",
            post(invoke_list_versions),
        )
        .route(
            "/tools/skill_submit_draft/invoke",
            post(invoke_submit_draft),
        )
        .route("/tools/skill_promote/invoke", post(invoke_promote))
        .route(
            "/tools/skill_dependencies/invoke",
            post(invoke_dependencies),
        )
        .route("/tools/skill_usage/invoke", post(invoke_usage))
        .with_state(store)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn submit_draft(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<SubmitDraftRequest>,
) -> impl IntoResponse {
    match store.submit_draft(req).await {
        Ok(meta) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(meta).unwrap()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_skill_content(
    State(store): State<Arc<SkillStore>>,
    Path(id): Path<String>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    // Allow optional ?version= query param via the q field (reuse SearchQuery loosely)
    // Actually, let's use a dedicated extraction — but for simplicity, version comes from query
    let version = params.q.clone(); // reuse q as version hint if needed
    match store.read_skill(id, version).await {
        Ok(Some(content)) => Json(serde_json::to_value(content).unwrap()).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "skill not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn search_skills(
    State(store): State<Arc<SkillStore>>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let tag = params.tags.clone();
    match store
        .search(
            tag,
            params.q.clone(),
            params.status.clone(),
            params.scope.clone(),
            params.limit,
        )
        .await
    {
        Ok(results) => Json(serde_json::to_value(results).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn list_versions(
    State(store): State<Arc<SkillStore>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match store.list_versions(id).await {
        Ok(versions) => Json(serde_json::to_value(versions).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn promote_skill(
    State(store): State<Arc<SkillStore>>,
    Path((id, version)): Path<(String, String)>,
    Json(req): Json<PromoteRequest>,
) -> impl IntoResponse {
    match store.promote(id, version, req.target_status).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "promoted": true })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ─── MCP-style tool surface (S3.T1) ─────────────────────────────────────────

async fn list_tools() -> impl IntoResponse {
    Json(serde_json::json!({
        "tools": [
            {
                "name": "skill_search",
                "description": "Search skills by text, tags, status, or scope",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "q": {"type": "string"},
                        "tags": {"type": "string"},
                        "status": {"type": "string"},
                        "scope": {"type": "string"},
                        "limit": {"type": "integer"}
                    }
                }
            },
            {
                "name": "skill_read",
                "description": "Read full skill content by id and optional version",
                "input_schema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {"type": "string"},
                        "version": {"type": "string"}
                    }
                }
            },
            {
                "name": "skill_list_versions",
                "description": "List all versions of a skill",
                "input_schema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {"id": {"type": "string"}}
                }
            },
            {
                "name": "skill_submit_draft",
                "description": "Submit a new skill draft",
                "input_schema": {
                    "type": "object",
                    "required": ["id", "name", "description", "version", "frontmatter_yaml", "prose"],
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"}
                    }
                }
            },
            {
                "name": "skill_promote",
                "description": "Promote a skill version to a new status",
                "input_schema": {
                    "type": "object",
                    "required": ["id", "version", "target_status"],
                    "properties": {
                        "id": {"type": "string"},
                        "version": {"type": "string"},
                        "target_status": {"type": "string"}
                    }
                }
            },
            {
                "name": "skill_dependencies",
                "description": "Read the declared dependencies of a skill",
                "input_schema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {"type": "string"},
                        "version": {"type": "string"}
                    }
                }
            },
            {
                "name": "skill_usage",
                "description": "Fetch usage telemetry for a skill (stub, pending Phase 1 L3 telemetry)",
                "input_schema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {"id": {"type": "string"}}
                }
            }
        ]
    }))
}

async fn invoke_skill_search(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<InvokeSkillSearch>,
) -> impl IntoResponse {
    match store
        .search(req.tags, req.q, req.status, req.scope, req.limit)
        .await
    {
        Ok(results) => Json(serde_json::to_value(results).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_skill_read(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<InvokeSkillRead>,
) -> impl IntoResponse {
    match store.read_skill(req.id, req.version).await {
        Ok(Some(content)) => Json(serde_json::to_value(content).unwrap()).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "skill not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_list_versions(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<InvokeById>,
) -> impl IntoResponse {
    match store.list_versions(req.id).await {
        Ok(versions) => Json(serde_json::to_value(versions).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_submit_draft(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<SubmitDraftRequest>,
) -> impl IntoResponse {
    match store.submit_draft(req).await {
        Ok(meta) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(meta).unwrap()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_promote(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<InvokePromote>,
) -> impl IntoResponse {
    match store.promote(req.id, req.version, req.target_status).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "promoted": true })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_dependencies(
    State(store): State<Arc<SkillStore>>,
    Json(req): Json<InvokeById>,
) -> impl IntoResponse {
    match store.read_skill(req.id, req.version).await {
        Ok(Some(content)) => {
            let deps = content
                .parsed_v2
                .as_ref()
                .map(|v| v.dependencies.clone())
                .unwrap_or_default();
            Json(serde_json::json!({ "dependencies": deps })).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "skill not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn invoke_usage(
    State(_store): State<Arc<SkillStore>>,
    Json(_req): Json<InvokeById>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "session_count": 0,
        "last_used": serde_json::Value::Null,
        "note": "Usage tracking pending Phase 1 L3 telemetry ingest (Deferred D9)"
    }))
}
