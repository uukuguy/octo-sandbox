use std::sync::Arc;

use axum::{routing::get, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api;
use crate::state::AppState;
use crate::ws::ws_handler;

async fn health() -> &'static str {
    "ok"
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Root level routes
        .route("/api/health", get(health))
        .route("/ws", get(ws_handler))
        // API routes - parameterized paths BEFORE fixed paths
        .route("/api/sessions/{id}/executions", get(api::executions::list_session_executions))
        .route("/api/sessions/{id}", get(api::sessions::get_session))
        .route("/api/executions/{id}", get(api::executions::get_execution))
        // Fixed paths last
        .route("/api/sessions", get(api::sessions::list_sessions))
        .route("/api/tools", get(api::tools::list_tools))
        .route("/api/memories", get(api::memories::search_memories))
        .route("/api/memories/working", get(api::memories::get_working_memory))
        .route("/api/budget", get(api::budget::get_budget))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
