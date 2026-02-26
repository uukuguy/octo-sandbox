use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use octo_types::{SessionId, ToolExecution};

use crate::state::AppState;
use crate::ws::ws_handler;

async fn health() -> &'static str {
    "ok"
}

// ============ Sessions API ============

#[derive(Debug, serde::Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

impl PaginationParams {
    pub fn clamped(&self) -> (usize, usize) {
        (self.limit.min(200).max(1), self.offset)
    }
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Json<serde_json::Value> {
    let limit = params.limit.min(100);
    let summaries = state.sessions.list_sessions(limit, params.offset).await;
    Json(serde_json::to_value(summaries).unwrap_or_default())
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let session_id = SessionId::from_string(&id);
    let messages = state.sessions.get_messages(&session_id).await;
    Json(serde_json::json!({
        "id": id,
        "messages": messages.unwrap_or_default(),
    }))
}

// ============ Executions API ============

async fn list_session_executions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Json<Vec<ToolExecution>> {
    let (limit, offset) = params.clamped();
    match &state.recorder {
        Some(recorder) => {
            let execs = recorder
                .list_by_session(&session_id, limit, offset)
                .await
                .unwrap_or_default();
            Json(execs)
        }
        None => Json(vec![]),
    }
}

async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match &state.recorder {
        Some(recorder) => {
            let exec = recorder.get(&id).await.ok().flatten();
            Json(serde_json::to_value(exec).unwrap_or_default())
        }
        None => Json(serde_json::json!(null)),
    }
}

// ============ Tools API ============

#[derive(serde::Serialize)]
struct ToolInfo {
    name: String,
    description: String,
    source: octo_types::ToolSource,
}

async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<ToolInfo>> {
    use octo_types::ToolSource;
    let specs = state.tools.specs();
    let tools: Vec<ToolInfo> = specs
        .into_iter()
        .map(|spec| {
            let source = state
                .tools
                .get(&spec.name)
                .map(|t| t.source())
                .unwrap_or(ToolSource::BuiltIn);
            ToolInfo {
                name: spec.name,
                description: spec.description,
                source,
            }
        })
        .collect();
    Json(tools)
}

// ============ Memories API ============

#[derive(Debug, serde::Deserialize)]
pub struct MemorySearchParams {
    pub query: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

async fn search_memories(
    State(_state): State<Arc<AppState>>,
    Query(_params): Query<MemorySearchParams>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "results": [] }))
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkingMemoryParams {
    pub sandbox_id: Option<String>,
}

async fn get_working_memory(
    State(_state): State<Arc<AppState>>,
    Query(_params): Query<WorkingMemoryParams>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "blocks": [] }))
}

// ============ Budget API ============

use octo_types::TokenBudgetSnapshot;

async fn get_budget(State(_state): State<Arc<AppState>>) -> Json<TokenBudgetSnapshot> {
    Json(TokenBudgetSnapshot {
        total: 100000,
        system_prompt: 0,
        dynamic_context: 0,
        history: 0,
        free: 100000,
        usage_percent: 0.0,
        degradation_level: 0,
    })
}

// ============ Router ============

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // API routes under /api prefix - Axum 0.8 fixes the parameterized route bug
    let api = Router::new()
        // Fixed paths first
        .route("/sessions", get(list_sessions))
        .route("/tools", get(list_tools))
        .route("/memories", get(search_memories))
        .route("/memories/working", get(get_working_memory))
        .route("/budget", get(get_budget))
        // Parameterized paths
        .route("/sessions/{id}/executions", get(list_session_executions))
        .route("/sessions/{id}", get(get_session))
        .route("/executions/{id}", get(get_execution));

    Router::new()
        .route("/api/health", get(health))
        .route("/ws", get(ws_handler))
        .nest("/api", api)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
