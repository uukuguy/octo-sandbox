use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::tools::ToolRegistry;

use super::bridge::McpToolBridge;
use super::sse::SseMcpClient;
use super::stdio::StdioMcpClient;
use super::traits::{McpClient, McpServerConfig, McpServerConfigV2, McpToolInfo, McpTransport};

/// MCP config file format (.octo/mcp.json).
#[derive(Debug, serde::Deserialize)]
struct McpConfigFile {
    servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct McpServerEntry {
    command: String,
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

/// Runtime state of an MCP server.
#[derive(Debug, Clone)]
pub enum ServerRuntimeState {
    Stopped,
    Starting,
    Running { pid: u32 },
    Error { message: String },
}

/// Manages multiple MCP server connections.
pub struct McpManager {
    clients: HashMap<String, Arc<RwLock<Box<dyn McpClient>>>>,
    tool_infos: HashMap<String, Vec<McpToolInfo>>,
    runtime_states: HashMap<String, ServerRuntimeState>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            tool_infos: HashMap::new(),
            runtime_states: HashMap::new(),
        }
    }

    /// Set runtime state.
    pub fn set_runtime_state(&mut self, name: &str, state: ServerRuntimeState) {
        self.runtime_states.insert(name.to_string(), state);
    }

    /// Get runtime state.
    pub fn get_runtime_state(&self, name: &str) -> ServerRuntimeState {
        self.runtime_states
            .get(name)
            .cloned()
            .unwrap_or(ServerRuntimeState::Stopped)
    }

    /// Get all runtime states.
    pub fn all_runtime_states(&self) -> HashMap<String, ServerRuntimeState> {
        self.runtime_states.clone()
    }

    /// Load MCP server configs from a JSON file.
    pub fn load_config(config_path: &Path) -> Result<Vec<McpServerConfig>> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let config: McpConfigFile = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", config_path.display()))?;

        Ok(config
            .servers
            .into_iter()
            .map(|(name, entry)| McpServerConfig {
                name,
                command: entry.command,
                args: entry.args,
                env: entry.env,
            })
            .collect())
    }

    /// Add and connect a new MCP server.
    pub async fn add_server(&mut self, config: McpServerConfig) -> Result<Vec<McpToolInfo>> {
        let name = config.name.clone();
        let mut client = StdioMcpClient::new(config);
        client.connect().await?;

        let tools = client.list_tools().await?;
        info!(
            server = %name,
            tool_count = tools.len(),
            "MCP server connected with tools"
        );

        let client: Arc<RwLock<Box<dyn McpClient>>> =
            Arc::new(RwLock::new(Box::new(client)));
        self.clients.insert(name.clone(), client);
        self.tool_infos.insert(name, tools.clone());
        Ok(tools)
    }

    /// Add and connect a new MCP server, supporting both Stdio and SSE transports.
    pub async fn add_server_v2(&mut self, config: McpServerConfigV2) -> Result<Vec<McpToolInfo>> {
        let name = config.name.clone();

        let mut client: Box<dyn McpClient> = match config.transport {
            McpTransport::Stdio => Box::new(StdioMcpClient::new(McpServerConfig {
                name: config.name.clone(),
                command: config.command.clone(),
                args: config.args.clone(),
                env: config.env.clone(),
            })),
            McpTransport::Sse => {
                let url = config.url.clone().ok_or_else(|| {
                    anyhow::anyhow!("SSE transport requires 'url' field for server '{name}'")
                })?;
                Box::new(SseMcpClient::new(config.name.clone(), url))
            }
        };

        client.connect().await?;
        let tools = client.list_tools().await?;

        info!(
            server = %name,
            transport = ?config.transport,
            tool_count = tools.len(),
            "MCP server connected with tools"
        );

        let client: Arc<RwLock<Box<dyn McpClient>>> = Arc::new(RwLock::new(client));
        self.clients.insert(name.clone(), client);
        self.tool_infos.insert(name, tools.clone());
        Ok(tools)
    }

    /// Remove and shutdown an MCP server.
    pub async fn remove_server(&mut self, name: &str) -> Result<()> {
        if let Some(client) = self.clients.remove(name) {
            let mut client = client.write().await;
            client.shutdown().await?;
        }
        self.tool_infos.remove(name);
        info!(server = %name, "MCP server removed");
        Ok(())
    }

    /// Bridge all MCP tools into a ToolRegistry.
    pub fn bridge_tools(&self, registry: &mut ToolRegistry) {
        for (server_name, tools) in &self.tool_infos {
            let client = self.clients.get(server_name).unwrap().clone();
            for tool_info in tools {
                let bridge = McpToolBridge::new(
                    client.clone(),
                    server_name.clone(),
                    tool_info.clone(),
                );
                registry.register(bridge);
                debug!(
                    server = %server_name,
                    tool = %tool_info.name,
                    "Bridged MCP tool"
                );
            }
        }
    }

    /// Shutdown all MCP servers.
    pub async fn shutdown_all(&mut self) -> Result<()> {
        let names: Vec<String> = self.clients.keys().cloned().collect();
        for name in names {
            if let Some(client) = self.clients.remove(&name) {
                let mut c = client.write().await;
                if let Err(e) = c.shutdown().await {
                    warn!(server = %name, error = %e, "Error shutting down MCP server");
                }
            }
        }
        self.tool_infos.clear();
        Ok(())
    }

    /// Get number of connected servers.
    pub fn server_count(&self) -> usize {
        self.clients.len()
    }

    /// Get tool infos for a server.
    pub fn get_tool_infos(&self, name: &str) -> Option<Vec<McpToolInfo>> {
        self.tool_infos.get(name).cloned()
    }

    /// Get tool count for a server.
    pub fn get_tool_count(&self, name: &str) -> usize {
        self.tool_infos.get(name).map(|t| t.len()).unwrap_or(0)
    }

    /// Call a tool on a server.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
        let client = client.read().await;
        client.call_tool(tool_name, args).await
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
