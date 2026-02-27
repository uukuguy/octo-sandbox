pub mod budget;
pub mod executions;
pub mod memories;
pub mod sessions;
pub mod tools;

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
        .route("/sessions/{id}/executions", get(executions::list_session_executions))
        .route("/sessions/{id}", get(sessions::get_session))
        // Then less specific
        .route("/sessions", get(sessions::list_sessions))
        .route("/executions/{id}", get(executions::get_execution))
        .route("/tools", get(tools::list_tools))
        .route("/memories", get(memories::search_memories).delete(memories::delete_memories_by_filter))
        .route("/memories/working", get(memories::get_working_memory))
        .route("/memories/{id}", get(memories::get_memory).delete(memories::delete_memory))
        .route("/budget", get(budget::get_budget))
}
