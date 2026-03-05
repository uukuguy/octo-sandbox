//! MCP configuration API for tenants

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{ArcAppState, AuthExtractor, ErrorResponse};

pub async fn list_mcp(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
) -> Result<Json<Vec<McpServerConfig>>, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime(&auth.tenant_id);
    let servers: Vec<McpServerConfig> = runtime
        .mcp_servers
        .iter()
        .map(|entry| McpServerConfig {
            id: entry.key().clone(),
            config: entry.value().clone(),
        })
        .collect();
    Ok(Json(servers))
}

pub async fn get_mcp(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(id): Path<String>,
) -> Result<Json<McpServerConfig>, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime(&auth.tenant_id);
    let entry = runtime.mcp_servers.get(&id);
    match entry {
        Some(entry) => {
            let id = entry.key().clone();
            let config = entry.value().clone();
            Ok(Json(McpServerConfig { id, config }))
        }
        None => Err(ErrorResponse {
            error: "MCP server not found".to_string(),
        }),
    }
}

pub async fn add_mcp(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Json(config): Json<serde_json::Value>,
) -> Result<Json<McpServerConfig>, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime(&auth.tenant_id);
    let id = uuid::Uuid::new_v4().to_string();
    runtime.mcp_servers.insert(id.clone(), config.clone());
    Ok(Json(McpServerConfig { id, config }))
}

pub async fn delete_mcp(
    State(state): State<ArcAppState>,
    auth: AuthExtractor,
    Path(id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let runtime = state.tenant_manager.get_or_create_runtime(&auth.tenant_id);
    if runtime.mcp_servers.remove(&id).is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorResponse {
            error: "MCP server not found".to_string(),
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct McpServerConfig {
    pub id: String,
    pub config: serde_json::Value,
}
