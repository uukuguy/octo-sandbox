pub mod agents;
pub mod audit;
pub mod budget;
pub mod config;
pub mod executions;
pub mod mcp_logs;
pub mod mcp_servers;
pub mod mcp_tools;
pub mod memories;
pub mod metrics;
pub mod providers;
pub mod scheduler;
pub mod sessions;
pub mod tasks;
pub mod tools;
pub mod user_context;

use std::sync::Arc;

use axum::{routing::get, Router};

use crate::state::AppState;

/// Pagination query params.
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

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // More specific routes first
        .route(
            "/sessions/{id}/executions",
            get(executions::list_session_executions),
        )
        .route("/sessions/{id}", get(sessions::get_session))
        // Then less specific
        .route("/sessions", get(sessions::list_sessions))
        .route("/executions", get(executions::list_user_executions))
        .route("/executions/{id}", get(executions::get_execution))
        .route("/tools", get(tools::list_tools))
        .route("/config", get(config::get_config))
        .route(
            "/memories",
            get(memories::search_memories)
                .post(memories::create_memory)
                .delete(memories::delete_memories_by_filter),
        )
        .route("/memories/working", get(memories::get_working_memory))
        .route(
            "/memories/{id}",
            get(memories::get_memory).delete(memories::delete_memory),
        )
        .route("/budget", get(budget::get_budget))
        // Metrics and Audit
        .merge(metrics::router())
        .merge(audit::router())
        // MCP servers
        .merge(mcp_servers::routes())
        .merge(mcp_tools::routes())
        .merge(mcp_logs::routes())
        // Scheduler
        .nest("/scheduler", scheduler::create_router())
        // Background tasks
        .merge(tasks::router())
        // Provider chain
        .merge(providers::router())
        // Agent registry and lifecycle
        .nest("/v1", agents::router())
}
