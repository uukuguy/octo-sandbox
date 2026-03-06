use std::sync::Arc;

use axum::{
    extract::{Extension, Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use octo_engine::auth::UserContext;
use octo_engine::mcp::{
    manager::ServerRuntimeState, storage::McpServerRecord, traits::McpServerConfig,
};
use serde::{Deserialize, Serialize};

use super::user_context::get_user_id_from_context;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerConfigRequest {
    pub name: String,
    pub source: Option<String>,
    // Stdio transport fields
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    // SSE transport fields
    pub transport: Option<String>, // "stdio" | "sse", defaults to "stdio"
    pub url: Option<String>,       // SSE only
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerResponse {
    pub id: String,
    pub name: String,
    pub source: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub transport: String,   // "stdio" | "sse"
    pub url: Option<String>, // SSE only
    pub enabled: bool,
    pub runtime_status: String,
    pub tool_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerStatusResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    pub pid: Option<u32>,
    pub error: Option<String>,
    pub tool_count: usize,
}

// List all MCP servers for the current user
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Vec<McpServerResponse>> {
    let user_id = get_user_id_from_context(Some(&ctx));

    let Some(storage) = state.mcp_storage() else {
        return Json(vec![]);
    };

    let runtime_states = state.agent_supervisor.list_mcp_servers().await;

    // Use list_servers_for_user to filter by user_id
    match storage.list_servers_for_user(&user_id) {
        Ok(records) => {
            let responses: Vec<McpServerResponse> = records
                .into_iter()
                .map(|r| {
                    // Find runtime state for this server
                    let runtime_status = runtime_states
                        .iter()
                        .find(|state| match state {
                            octo_engine::mcp::manager::ServerRuntimeState::Running { .. } => true,
                            _ => false,
                        })
                        .map(|_| "running")
                        .unwrap_or("stopped");

                    // Tool count is managed by AgentRuntime - return 0 for now
                    // The actual tool count is tracked internally by AgentRuntime
                    let tool_count = 0;

                    // Default to stdio transport for backward compatibility
                    let transport = r.transport.unwrap_or_else(|| "stdio".to_string());

                    McpServerResponse {
                        id: r.id,
                        name: r.name,
                        source: r.source,
                        command: r.command,
                        args: r.args.split_whitespace().map(String::from).collect(),
                        env: serde_json::from_str(&r.env).unwrap_or_default(),
                        transport,
                        url: r.url,
                        enabled: r.enabled,
                        runtime_status: runtime_status.to_string(),
                        tool_count,
                        created_at: r.created_at,
                        updated_at: r.updated_at,
                    }
                })
                .collect();
            Json(responses)
        }
        Err(e) => {
            tracing::error!("Failed to list MCP servers: {e}");
            Json(vec![])
        }
    }
}

// Get single server
pub async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Option<McpServerResponse>> {
    let user_id = get_user_id_from_context(Some(&ctx));

    let Some(storage) = state.mcp_storage() else {
        return Json(None);
    };

    // Use get_server_for_user to filter by user_id
    match storage.get_server_for_user(&id, &user_id) {
        Ok(Some(r)) => Json(Some(McpServerResponse {
            id: r.id,
            name: r.name,
            source: r.source,
            command: r.command,
            args: r.args.split_whitespace().map(String::from).collect(),
            env: serde_json::from_str(&r.env).unwrap_or_default(),
            transport: "stdio".to_string(),
            url: None,
            enabled: r.enabled,
            runtime_status: "stopped".to_string(),
            tool_count: 0,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })),
        Ok(None) => Json(None),
        Err(e) => {
            tracing::error!("Failed to get MCP server: {e}");
            Json(None)
        }
    }
}

// Create new server
pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<UserContext>,
    Json(req): Json<McpServerConfigRequest>,
) -> Json<McpServerResponse> {
    let user_id = get_user_id_from_context(Some(&ctx));

    let Some(storage) = state.mcp_storage() else {
        return Json(McpServerResponse {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            source: req.source.unwrap_or_else(|| "manual".to_string()),
            command: req.command.unwrap_or_default(),
            args: req.args.unwrap_or_default(),
            env: req.env.unwrap_or_default(),
            transport: req.transport.unwrap_or_else(|| "stdio".to_string()),
            url: req.url,
            enabled: req.enabled.unwrap_or(true),
            runtime_status: "stopped".to_string(),
            tool_count: 0,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        });
    };

    let now = Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let transport = req.transport.as_deref().unwrap_or("stdio").to_string();
    let source = req.source.unwrap_or_else(|| "manual".to_string());
    let command = req.command.unwrap_or_default();
    let args_vec = req.args.unwrap_or_default();
    let args_str = args_vec.join(" ");
    let env_map = req.env.unwrap_or_default();
    let env_str = serde_json::to_string(&env_map).unwrap_or_default();
    let enabled = req.enabled.unwrap_or(true);

    let record = octo_engine::mcp::storage::McpServerRecord {
        id: id.clone(),
        name: req.name.clone(),
        source: source.clone(),
        command: command.clone(),
        args: args_str.clone(),
        env: env_str,
        enabled,
        transport: Some(transport.clone()),
        url: req.url.clone(),
        user_id: user_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    if let Err(e) = storage.insert_server(&record) {
        tracing::error!("Failed to create MCP server: {e}");
    }

    Json(McpServerResponse {
        id,
        name: req.name,
        source,
        command,
        args: args_vec,
        env: env_map,
        transport,
        url: req.url,
        enabled,
        runtime_status: "stopped".to_string(),
        tool_count: 0,
        created_at: now.clone(),
        updated_at: now,
    })
}

// Update server
pub async fn update_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
    Json(req): Json<McpServerConfigRequest>,
) -> Json<Option<McpServerResponse>> {
    let user_id = get_user_id_from_context(Some(&ctx));

    let storage = match state.mcp_storage() {
        Some(s) => s,
        None => return Json(None),
    };

    // Get existing server to preserve created_at and verify ownership
    let existing = storage.get_server_for_user(&id, &user_id).ok().flatten();
    if existing.is_none() {
        return Json(None);
    }
    let existing = existing.unwrap();
    let created_at = existing.created_at;

    let now = chrono::Utc::now().to_rfc3339();
    let env_str = req
        .env
        .as_ref()
        .map(|e| {
            e.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();

    let record = McpServerRecord {
        id: id.clone(),
        name: req.name,
        source: req.source.unwrap_or_else(|| "custom".to_string()),
        command: req.command.unwrap_or_default(),
        args: req.args.map(|a| a.join(" ")).unwrap_or_default(),
        env: env_str,
        enabled: req.enabled.unwrap_or(true),
        transport: req.transport.clone(),
        url: req.url.clone(),
        user_id: user_id.clone(),
        created_at,
        updated_at: now,
    };

    match storage.update_server(&record) {
        Ok(_) => {
            // Get runtime status from manager
            let runtime = state.agent_supervisor.get_mcp_runtime_state(&id).await;
            let runtime_status = match runtime {
                ServerRuntimeState::Stopped => "stopped",
                ServerRuntimeState::Starting => "starting",
                ServerRuntimeState::Running { .. } => "running",
                ServerRuntimeState::Error { .. } => "error",
            }
            .to_string();

            Json(Some(McpServerResponse {
                id: record.id,
                name: record.name,
                source: record.source,
                command: record.command,
                args: record.args.split_whitespace().map(String::from).collect(),
                env: req.env.unwrap_or_default(),
                enabled: record.enabled,
                transport: record.transport.unwrap_or_else(|| "stdio".to_string()),
                url: record.url,
                runtime_status,
                tool_count: 0,
                created_at: record.created_at,
                updated_at: record.updated_at,
            }))
        }
        Err(_) => Json(None),
    }
}

// Delete server
pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<serde_json::Value> {
    let user_id = get_user_id_from_context(Some(&ctx));

    if let Some(storage) = state.mcp_storage() {
        // First check if the server belongs to the user
        let existing = storage.get_server_for_user(&id, &user_id).ok().flatten();
        if existing.is_none() {
            return Json(serde_json::json!({"error": "Server not found or access denied"}));
        }

        if let Err(e) = storage.delete_server(&id) {
            tracing::error!("Failed to delete MCP server: {e}");
            return Json(serde_json::json!({"error": e.to_string()}));
        }
    }
    Json(serde_json::json!({"deleted": id}))
}

// Start server
pub async fn start_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<serde_json::Value> {
    let user_id = get_user_id_from_context(Some(&ctx));

    // Get server config from storage
    let Some(storage) = state.mcp_storage() else {
        return Json(serde_json::json!({"error": "MCP storage not available"}));
    };

    // Verify server belongs to user
    let server_record = match storage.get_server_for_user(&id, &user_id) {
        Ok(Some(record)) => record,
        Ok(None) => {
            return Json(serde_json::json!({"error": "Server not found or access denied"}));
        }
        Err(e) => {
            return Json(serde_json::json!({"error": format!("Failed to get server: {}", e)}));
        }
    };

    // Create McpServerConfig from storage record (convert from V2 to simple config)
    let config = McpServerConfig {
        name: server_record.name.clone(),
        command: server_record.command.clone(),
        args: serde_json::from_str(&server_record.args).unwrap_or_default(),
        env: serde_json::from_str(&server_record.env).unwrap_or_default(),
    };

    // Add and connect the server via AgentRuntime
    match state.agent_supervisor.add_mcp_server(config).await {
        Ok(tools) => {
            tracing::info!(server = %id, tool_count = tools.len(), "MCP server started");
            Json(serde_json::json!({
                "started": id,
                "tool_count": tools.len()
            }))
        }
        Err(e) => {
            tracing::error!(server = %id, error = %e, "Failed to start MCP server");
            Json(serde_json::json!({"error": format!("Failed to start server: {}", e)}))
        }
    }
}

// Stop server
pub async fn stop_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<serde_json::Value> {
    let user_id = get_user_id_from_context(Some(&ctx));

    // Verify server belongs to user before stopping
    if let Some(storage) = state.mcp_storage() {
        let existing = storage.get_server_for_user(&id, &user_id).ok().flatten();
        if existing.is_none() {
            return Json(serde_json::json!({"error": "Server not found or access denied"}));
        }
    }

    // Use AgentRuntime to remove the MCP server
    match state.agent_supervisor.remove_mcp_server(&id).await {
        Ok(()) => {
            tracing::info!(server = %id, "MCP server stopped");
            Json(serde_json::json!({"stopped": id}))
        }
        Err(e) => {
            tracing::error!(server = %id, error = %e, "Failed to stop MCP server");
            Json(serde_json::json!({"error": format!("Failed to stop server: {}", e)}))
        }
    }
}

// Get server status
pub async fn get_server_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Option<McpServerStatusResponse>> {
    let user_id = get_user_id_from_context(Some(&ctx));

    // Verify server belongs to user before showing status
    if let Some(storage) = state.mcp_storage() {
        let existing = storage.get_server_for_user(&id, &user_id).ok().flatten();
        if existing.is_none() {
            return Json(None);
        }
    }

    let runtime_state = state.agent_supervisor.get_mcp_runtime_state(&id).await;
    let tool_count = state.agent_supervisor.get_mcp_tool_count(&id).await;

    let (status, pid, error) = match runtime_state {
        ServerRuntimeState::Running { pid: p } => ("running", Some(p), None),
        ServerRuntimeState::Stopped => ("stopped", None, None),
        ServerRuntimeState::Starting => ("starting", None, None),
        ServerRuntimeState::Error { message } => ("error", None, Some(message)),
    };

    Json(Some(McpServerStatusResponse {
        id: id.clone(),
        name: id,
        status: status.to_string(),
        pid,
        error,
        tool_count,
    }))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers", get(list_servers))
        .route("/mcp/servers", post(create_server))
        .route("/mcp/servers/{id}", get(get_server))
        .route("/mcp/servers/{id}", put(update_server))
        .route("/mcp/servers/{id}", delete(delete_server))
        .route("/mcp/servers/{id}/start", post(start_server))
        .route("/mcp/servers/{id}/stop", post(stop_server))
        .route("/mcp/servers/{id}/status", get(get_server_status))
}
