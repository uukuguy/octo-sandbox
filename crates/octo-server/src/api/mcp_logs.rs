use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LogQueryParams {
    pub level: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct McpLogEntry {
    pub id: String,
    pub server_id: String,
    pub level: String,
    pub direction: String,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub raw_data: Option<String>,
    pub duration_ms: Option<i64>,
    pub logged_at: String,
}

// List logs
pub async fn list_logs(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(params): Query<LogQueryParams>,
) -> Json<Vec<McpLogEntry>> {
    let limit = params.limit.unwrap_or(100);
    let offset = params.offset.unwrap_or(0);

    let storage = match state.mcp_storage() {
        Some(s) => s,
        None => return Json(vec![]),
    };

    let level = params.level.as_deref();
    match storage.list_logs(&server_id, level, limit, offset) {
        Ok(logs) => Json(logs.into_iter().map(|l| McpLogEntry {
            id: l.id,
            server_id: l.server_id,
            level: l.level,
            direction: l.direction,
            method: l.method,
            params: l.params.map(|p| serde_json::from_str(&p).unwrap_or_default()),
            result: l.result.map(|r| serde_json::from_str(&r).unwrap_or_default()),
            raw_data: l.raw_data,
            duration_ms: l.duration_ms,
            logged_at: l.logged_at,
        }).collect()),
        Err(_) => Json(vec![]),
    }
}

// Clear logs
pub async fn clear_logs(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<serde_json::Value> {
    let storage = match state.mcp_storage() {
        Some(s) => s,
        None => return Json(serde_json::json!({"error": "storage not available"})),
    };

    match storage.clear_logs(&server_id) {
        Ok(_) => Json(serde_json::json!({"cleared": server_id, "count": 0})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

// Export logs
pub async fn export_logs(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<serde_json::Value> {
    let storage = match state.mcp_storage() {
        Some(s) => s,
        None => return Json(serde_json::json!({"error": "storage not available"})),
    };

    // Get all logs (limit 10000 for export)
    match storage.list_logs(&server_id, None, 10000, 0) {
        Ok(logs) => Json(serde_json::json!({
            "exported": server_id,
            "format": "json",
            "count": logs.len(),
            "logs": logs
        })),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers/{server_id}/logs", get(list_logs))
        .route("/mcp/servers/{server_id}/logs", delete(clear_logs))
        .route("/mcp/servers/{server_id}/logs/export", get(export_logs))
}
