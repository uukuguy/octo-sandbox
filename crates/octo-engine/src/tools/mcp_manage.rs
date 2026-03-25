//! MCP server management tools — install/remove/list MCP servers from within agent conversations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;
use super::ToolRegistry;
use crate::mcp::bridge::McpToolBridge;
use crate::mcp::manager::McpManager;
use crate::mcp::stdio::StdioMcpClient;
use crate::mcp::traits::{McpClient, McpServerConfig, McpToolInfo};

// ── Shared handle passed to all three tools ──────────────────────

/// Shared references needed by MCP management tools.
#[derive(Clone)]
pub struct McpManageHandle {
    pub mcp_manager: Arc<Mutex<McpManager>>,
    pub tools: Arc<StdMutex<ToolRegistry>>,
    pub config_path: PathBuf,
}

// ── mcp_install ──────────────────────────────────────────────────

/// Install (add + connect) an MCP server at runtime and persist to config.
pub struct McpInstallTool {
    handle: McpManageHandle,
}

impl McpInstallTool {
    pub fn new(handle: McpManageHandle) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl Tool for McpInstallTool {
    fn name(&self) -> &str {
        "mcp_install"
    }

    fn description(&self) -> &str {
        "Install and connect an MCP server. The server process is started, its tools are discovered and registered for immediate use, and the configuration is persisted so it auto-loads on next startup. Example: {\"name\": \"context7\", \"command\": \"npx\", \"args\": [\"-y\", \"@upstash/context7-mcp\"]}"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique server name (e.g. \"context7\", \"tavily\")"
                },
                "command": {
                    "type": "string",
                    "description": "Executable command (e.g. \"npx\", \"uvx\", \"docker\")"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command arguments (e.g. [\"-y\", \"@upstash/context7-mcp\"])"
                },
                "env": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Optional environment variables for the server process"
                }
            },
            "required": ["name", "command"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name' parameter"))?
            .to_string();
        let command = params["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?
            .to_string();
        let args: Vec<String> = params["args"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let env: HashMap<String, String> = params["env"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let config = McpServerConfig {
            name: name.clone(),
            command: command.clone(),
            args: args.clone(),
            env,
        };

        // 1. Persist to config file
        if let Err(e) = McpManager::add_to_config_file(&self.handle.config_path, &config) {
            tracing::warn!(error = %e, "Failed to persist MCP config (server will still be connected for this session)");
        }

        // 2. Connect server (outside manager lock for slow subprocess startup)
        {
            let mut guard = self.handle.mcp_manager.lock().await;
            guard.set_runtime_state(
                &name,
                crate::mcp::manager::ServerRuntimeState::Starting,
            );
        }

        let mut client = StdioMcpClient::new(config);
        if let Err(e) = client.connect().await {
            let mut guard = self.handle.mcp_manager.lock().await;
            guard.set_runtime_state(
                &name,
                crate::mcp::manager::ServerRuntimeState::Error {
                    message: e.to_string(),
                },
            );
            return Ok(ToolOutput::error(format!(
                "Failed to connect to MCP server '{}': {}",
                name, e
            )));
        }

        let tools: Vec<McpToolInfo> = match client.list_tools().await {
            Ok(t) => t,
            Err(e) => {
                let mut guard = self.handle.mcp_manager.lock().await;
                guard.set_runtime_state(
                    &name,
                    crate::mcp::manager::ServerRuntimeState::Error {
                        message: e.to_string(),
                    },
                );
                return Ok(ToolOutput::error(format!(
                    "Connected to '{}' but failed to list tools: {}",
                    name, e
                )));
            }
        };

        let client_arc: Arc<tokio::sync::RwLock<Box<dyn McpClient>>> =
            Arc::new(tokio::sync::RwLock::new(Box::new(client)));

        // 3. Register in McpManager
        {
            let mut guard = self.handle.mcp_manager.lock().await;
            guard.insert_connected_client(name.clone(), client_arc.clone(), tools.clone());
        }

        // 4. Bridge tools into ToolRegistry
        {
            let mut tools_guard = self
                .handle
                .tools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for tool_info in &tools {
                let bridge =
                    McpToolBridge::new(client_arc.clone(), name.clone(), tool_info.clone());
                tools_guard.register(bridge);
            }
        }

        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        Ok(ToolOutput::success(format!(
            "Installed MCP server '{}' ({} {} {}) with {} tool(s): {}",
            name,
            command,
            args.join(" "),
            if self.handle.config_path.exists() {
                format!("— persisted to {}", self.handle.config_path.display())
            } else {
                String::new()
            },
            tools.len(),
            tool_names.join(", ")
        )))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn execution_timeout(&self) -> std::time::Duration {
        // npx downloads can be slow
        std::time::Duration::from_secs(120)
    }

    fn category(&self) -> &str {
        "mcp"
    }
}

// ── mcp_remove ───────────────────────────────────────────────────

/// Remove an MCP server — disconnect, unregister tools, and remove from config.
pub struct McpRemoveTool {
    handle: McpManageHandle,
}

impl McpRemoveTool {
    pub fn new(handle: McpManageHandle) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl Tool for McpRemoveTool {
    fn name(&self) -> &str {
        "mcp_remove"
    }

    fn description(&self) -> &str {
        "Remove an MCP server — disconnect it, unregister its tools, and remove from config."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Server name to remove"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name' parameter"))?
            .to_string();

        // 1. Get tool names to unregister
        let removed_tool_names: Vec<String> = {
            let guard = self.handle.mcp_manager.lock().await;
            guard
                .get_tool_infos(&name)
                .map(|tools| tools.iter().map(|t| t.name.clone()).collect())
                .unwrap_or_default()
        };

        if removed_tool_names.is_empty() {
            return Ok(ToolOutput::error(format!(
                "MCP server '{}' not found",
                name
            )));
        }

        // 2. Disconnect from McpManager
        {
            let mut guard = self.handle.mcp_manager.lock().await;
            if let Err(e) = guard.remove_server(&name).await {
                return Ok(ToolOutput::error(format!(
                    "Failed to remove MCP server '{}': {}",
                    name, e
                )));
            }
        }

        // 3. Unregister tools from ToolRegistry (rebuild without this server's tools)
        {
            let mut tools_guard = self
                .handle
                .tools
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let all: Vec<(String, Arc<dyn Tool>)> = tools_guard
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let mut new_registry = ToolRegistry::new();
            for (tool_name, tool) in all {
                if matches!(tool.source(), ToolSource::Mcp(ref sn) if sn == &name) {
                    continue;
                }
                new_registry.register_arc(tool_name, tool);
            }
            *tools_guard = new_registry;
        }

        // 4. Remove from config file
        let _ = McpManager::remove_from_config_file(&self.handle.config_path, &name);

        Ok(ToolOutput::success(format!(
            "Removed MCP server '{}' ({} tools unregistered: {})",
            name,
            removed_tool_names.len(),
            removed_tool_names.join(", ")
        )))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn category(&self) -> &str {
        "mcp"
    }
}

// ── mcp_list ─────────────────────────────────────────────────────

/// List currently connected MCP servers and their tools.
pub struct McpListTool {
    handle: McpManageHandle,
}

impl McpListTool {
    pub fn new(handle: McpManageHandle) -> Self {
        Self { handle }
    }
}

#[async_trait]
impl Tool for McpListTool {
    fn name(&self) -> &str {
        "mcp_list"
    }

    fn description(&self) -> &str {
        "List all connected MCP servers, their status, and available tools."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let guard = self.handle.mcp_manager.lock().await;
        let states = guard.all_runtime_states();

        if states.is_empty() {
            return Ok(ToolOutput::success(
                "No MCP servers configured. Use mcp_install to add one.",
            ));
        }

        let mut lines = Vec::new();
        for (name, state) in &states {
            let tool_names: Vec<String> = guard
                .get_tool_infos(name)
                .unwrap_or_default()
                .iter()
                .map(|t| t.name.clone())
                .collect();
            lines.push(format!(
                "- {} [{:?}] ({} tools: {})",
                name,
                state,
                tool_names.len(),
                if tool_names.is_empty() {
                    "none".to_string()
                } else {
                    tool_names.join(", ")
                }
            ));
        }

        Ok(ToolOutput::success(format!(
            "{} MCP server(s):\n{}",
            states.len(),
            lines.join("\n")
        )))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn category(&self) -> &str {
        "mcp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_install_tool_metadata() {
        let handle = McpManageHandle {
            mcp_manager: Arc::new(Mutex::new(McpManager::new())),
            tools: Arc::new(StdMutex::new(ToolRegistry::new())),
            config_path: PathBuf::from("/tmp/test-mcp.json"),
        };
        let tool = McpInstallTool::new(handle);
        assert_eq!(tool.name(), "mcp_install");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::HighRisk);
        assert_eq!(tool.category(), "mcp");
        assert_eq!(tool.execution_timeout(), std::time::Duration::from_secs(120));
        let params = tool.parameters();
        assert!(params["properties"]["name"].is_object());
        assert!(params["properties"]["command"].is_object());
        assert!(params["properties"]["args"].is_object());
    }

    #[test]
    fn test_mcp_remove_tool_metadata() {
        let handle = McpManageHandle {
            mcp_manager: Arc::new(Mutex::new(McpManager::new())),
            tools: Arc::new(StdMutex::new(ToolRegistry::new())),
            config_path: PathBuf::from("/tmp/test-mcp.json"),
        };
        let tool = McpRemoveTool::new(handle);
        assert_eq!(tool.name(), "mcp_remove");
        assert_eq!(tool.category(), "mcp");
    }

    #[test]
    fn test_mcp_list_tool_metadata() {
        let handle = McpManageHandle {
            mcp_manager: Arc::new(Mutex::new(McpManager::new())),
            tools: Arc::new(StdMutex::new(ToolRegistry::new())),
            config_path: PathBuf::from("/tmp/test-mcp.json"),
        };
        let tool = McpListTool::new(handle);
        assert_eq!(tool.name(), "mcp_list");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_mcp_list_empty() {
        let handle = McpManageHandle {
            mcp_manager: Arc::new(Mutex::new(McpManager::new())),
            tools: Arc::new(StdMutex::new(ToolRegistry::new())),
            config_path: PathBuf::from("/tmp/test-mcp.json"),
        };
        let tool = McpListTool::new(handle);
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("No MCP servers"));
    }
}
