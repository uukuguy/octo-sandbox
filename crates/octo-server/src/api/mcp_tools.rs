use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use octo_engine::mcp::McpToolInfo;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCallResponse {
    pub id: String,
    pub server_id: String,
    pub tool_name: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub executed_at: String,
}

// List tools for a server
pub async fn list_tools(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<Vec<McpToolInfo>> {
    let tools = state.agent_supervisor.get_mcp_tool_infos(&server_id).await;
    Json(tools)
}

// Call a tool
pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(req): Json<McpToolCallRequest>,
) -> Json<McpToolCallResponse> {
    let start = std::time::Instant::now();
    let now = chrono::Utc::now();

    let result = state
        .agent_supervisor
        .call_mcp_tool(&server_id, &req.tool_name, req.arguments)
        .await;

    let duration_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok(result) => Json(McpToolCallResponse {
            id: uuid::Uuid::new_v4().to_string(),
            server_id,
            tool_name: req.tool_name,
            result: Some(result),
            error: None,
            duration_ms,
            executed_at: now.to_rfc3339(),
        }),
        Err(e) => Json(McpToolCallResponse {
            id: uuid::Uuid::new_v4().to_string(),
            server_id,
            tool_name: req.tool_name,
            result: None,
            error: Some(e),
            duration_ms,
            executed_at: now.to_rfc3339(),
        }),
    }
}

// List execution history
pub async fn list_executions(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<Vec<McpToolCallResponse>> {
    let storage = match state.mcp_storage() {
        Some(s) => s,
        None => return Json(vec![]),
    };

    match storage.list_executions(&server_id, 100) {
        Ok(execs) => Json(
            execs
                .into_iter()
                .map(|e| McpToolCallResponse {
                    id: e.id,
                    server_id: e.server_id,
                    tool_name: e.tool_name,
                    result: e
                        .result
                        .map(|r| serde_json::from_str(&r).unwrap_or_default()),
                    error: e.error,
                    duration_ms: e.duration_ms.unwrap_or(0),
                    executed_at: e.executed_at,
                })
                .collect(),
        ),
        Err(_) => Json(vec![]),
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers/{server_id}/tools", get(list_tools))
        .route("/mcp/servers/{server_id}/call", post(call_tool))
        .route("/mcp/servers/{server_id}/executions", get(list_executions))
}
