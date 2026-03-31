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
use super::traits::{
    McpClient, McpPromptInfo, McpPromptResult, McpResourceContent, McpResourceInfo,
    McpServerConfig, McpServerConfigV2, McpToolInfo, McpTransport,
};

/// MCP config file format — supports both octo and Claude Code formats.
///
/// Octo format:     `{ "servers": { ... } }`
/// CC format:       `{ "mcpServers": { ... } }`
/// Both supported simultaneously; entries are merged (CC takes precedence on conflict).
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct McpConfigFile {
    /// Octo-native format key.
    #[serde(default)]
    servers: HashMap<String, McpServerEntry>,
    /// Claude Code compatible format key.
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct McpServerEntry {
    command: String,
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    /// Transport type: "stdio" (default) or "http"/"sse".
    #[serde(default, rename = "type")]
    transport_type: Option<String>,
    /// URL for HTTP/SSE transport.
    #[serde(default)]
    url: Option<String>,
    /// Whether to auto-start on runtime init. Defaults to true.
    #[serde(default = "default_auto_start")]
    #[serde(rename = "autoStart")]
    auto_start: bool,
}

fn default_auto_start() -> bool {
    true
}

/// Expand `${VAR}` and `${VAR:-default}` references in a string using env vars.
fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_expr = String::new();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                var_expr.push(ch);
            }
            // Parse VAR:-default
            if let Some((var_name, default_val)) = var_expr.split_once(":-") {
                result.push_str(
                    &std::env::var(var_name).unwrap_or_else(|_| default_val.to_string()),
                );
            } else {
                result.push_str(&std::env::var(&var_expr).unwrap_or_default());
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Expand env vars in all string fields of an McpServerEntry.
fn expand_entry_env_vars(entry: &McpServerEntry) -> McpServerEntry {
    McpServerEntry {
        command: expand_env_vars(&entry.command),
        args: entry.args.iter().map(|a| expand_env_vars(a)).collect(),
        env: entry
            .env
            .iter()
            .map(|(k, v)| (k.clone(), expand_env_vars(v)))
            .collect(),
        transport_type: entry.transport_type.clone(),
        url: entry.url.as_ref().map(|u| expand_env_vars(u)),
        auto_start: entry.auto_start,
    }
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

    /// Get all clients (for creating tool bridges).
    pub fn clients(&self) -> &HashMap<String, Arc<RwLock<Box<dyn McpClient>>>> {
        &self.clients
    }

    /// Get all runtime states.
    pub fn all_runtime_states(&self) -> HashMap<String, ServerRuntimeState> {
        self.runtime_states.clone()
    }

    /// Load MCP server configs from a JSON file.
    ///
    /// Supports both octo format (`servers`) and Claude Code format (`mcpServers`).
    /// Environment variables (`${VAR}`, `${VAR:-default}`) are expanded in all string fields.
    pub fn load_config(config_path: &Path) -> Result<Vec<McpServerConfig>> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let config: McpConfigFile = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", config_path.display()))?;

        // Merge both keys: octo `servers` + CC `mcpServers` (CC wins on conflict)
        let mut merged: HashMap<String, McpServerEntry> = config.servers;
        merged.extend(config.mcp_servers);

        Ok(merged
            .into_iter()
            .map(|(name, entry)| {
                let auto_start = entry.auto_start;
                let expanded = expand_entry_env_vars(&entry);
                McpServerConfig {
                    name,
                    command: expanded.command,
                    args: expanded.args,
                    env: expanded.env,
                    auto_start,
                }
            })
            .collect())
    }

    /// Add a server entry to an mcp.json config file (creates if missing).
    ///
    /// Uses the CC-compatible `mcpServers` key for broad compatibility.
    pub fn add_to_config_file(config_path: &Path, config: &McpServerConfig) -> Result<()> {
        let mut file_config = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)
                .with_context(|| format!("reading {}", config_path.display()))?;
            serde_json::from_str::<McpConfigFile>(&content)
                .with_context(|| format!("parsing {}", config_path.display()))?
        } else {
            McpConfigFile {
                servers: HashMap::new(),
                mcp_servers: HashMap::new(),
            }
        };

        let entry = McpServerEntry {
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
            transport_type: None,
            url: None,
            auto_start: config.auto_start,
        };
        file_config.mcp_servers.insert(config.name.clone(), entry);

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(&file_config)?;
        std::fs::write(config_path, json)
            .with_context(|| format!("writing {}", config_path.display()))?;
        info!(path = %config_path.display(), server = %config.name, "Saved MCP server to config");
        Ok(())
    }

    /// Remove a server entry from an mcp.json config file.
    pub fn remove_from_config_file(config_path: &Path, name: &str) -> Result<bool> {
        if !config_path.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let mut file_config: McpConfigFile = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", config_path.display()))?;

        let removed_servers = file_config.servers.remove(name).is_some();
        let removed_mcp = file_config.mcp_servers.remove(name).is_some();

        if removed_servers || removed_mcp {
            let json = serde_json::to_string_pretty(&file_config)?;
            std::fs::write(config_path, json)
                .with_context(|| format!("writing {}", config_path.display()))?;
            info!(path = %config_path.display(), server = %name, "Removed MCP server from config");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Add and connect a new MCP server.
    /// NOTE: this method holds &mut self for the duration of connect() + list_tools().
    /// Prefer add_server_nonblocking() from AgentRuntime which connects outside the lock.
    pub async fn add_server(&mut self, config: McpServerConfig) -> Result<Vec<McpToolInfo>> {
        let name = config.name.clone();
        self.runtime_states
            .insert(name.clone(), ServerRuntimeState::Starting);

        let mut client = StdioMcpClient::new(config);
        match client.connect().await {
            Ok(()) => {}
            Err(e) => {
                self.runtime_states.insert(
                    name.clone(),
                    ServerRuntimeState::Error {
                        message: e.to_string(),
                    },
                );
                return Err(e);
            }
        }

        let tools = client.list_tools().await?;
        info!(
            server = %name,
            tool_count = tools.len(),
            "MCP server connected with tools"
        );

        let client: Arc<RwLock<Box<dyn McpClient>>> = Arc::new(RwLock::new(Box::new(client)));
        self.clients.insert(name.clone(), client);
        self.tool_infos.insert(name.clone(), tools.clone());
        self.runtime_states
            .insert(name, ServerRuntimeState::Running { pid: 0 });
        Ok(tools)
    }

    /// Insert an already-connected client without holding the lock during IO.
    /// Used by AgentRuntime::add_mcp_server to avoid lock contention.
    pub fn insert_connected_client(
        &mut self,
        name: String,
        client: Arc<RwLock<Box<dyn McpClient>>>,
        tools: Vec<McpToolInfo>,
    ) {
        self.clients.insert(name.clone(), client);
        self.tool_infos.insert(name.clone(), tools);
        self.runtime_states
            .insert(name, ServerRuntimeState::Running { pid: 0 });
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
                auto_start: true,
            })),
            McpTransport::Sse => {
                let url = config.url.clone().ok_or_else(|| {
                    anyhow::anyhow!("SSE transport requires 'url' field for server '{name}'")
                })?;
                if let Some(ref oauth) = config.oauth {
                    // If OAuth is configured, try to get a valid token
                    let oauth_mgr = super::oauth::McpOAuthManager::new();
                    let token_store = super::oauth::InMemoryTokenStore::new();
                    match oauth_mgr
                        .get_valid_token(oauth, &token_store, &config.id)
                        .await
                    {
                        Ok(Some(token)) => {
                            info!(server = %name, "Using OAuth token for SSE connection");
                            Box::new(SseMcpClient::with_auth(
                                config.name.clone(),
                                url,
                                token.bearer_value().to_string(),
                            ))
                        }
                        Ok(None) => {
                            warn!(
                                server = %name,
                                "OAuth configured but no valid token available; connecting without auth"
                            );
                            Box::new(SseMcpClient::new(config.name.clone(), url))
                        }
                        Err(e) => {
                            warn!(
                                server = %name,
                                error = %e,
                                "OAuth token retrieval failed; connecting without auth"
                            );
                            Box::new(SseMcpClient::new(config.name.clone(), url))
                        }
                    }
                } else {
                    Box::new(SseMcpClient::new(config.name.clone(), url))
                }
            }
        };

        self.runtime_states
            .insert(name.clone(), ServerRuntimeState::Starting);
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
        self.tool_infos.insert(name.clone(), tools.clone());
        self.runtime_states
            .insert(name, ServerRuntimeState::Running { pid: 0 });
        Ok(tools)
    }

    /// Remove and shutdown an MCP server.
    pub async fn remove_server(&mut self, name: &str) -> Result<()> {
        if let Some(client) = self.clients.remove(name) {
            let mut client = client.write().await;
            client.shutdown().await?;
        }
        self.tool_infos.remove(name);
        self.runtime_states.remove(name);
        info!(server = %name, "MCP server removed");
        Ok(())
    }

    /// Bridge all MCP tools into a ToolRegistry.
    pub fn bridge_tools(&self, registry: &mut ToolRegistry) {
        for (server_name, tools) in &self.tool_infos {
            let Some(client) = self.clients.get(server_name) else {
                warn!(server = %server_name, "tool_infos has entry but clients map does not; skipping bridge");
                continue;
            };
            let client = client.clone();
            for tool_info in tools {
                let bridge =
                    McpToolBridge::new(client.clone(), server_name.clone(), tool_info.clone());
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

    // --- Resources ---

    /// List available resources from a specific MCP server.
    pub async fn list_resources(&self, server_name: &str) -> Result<Vec<McpResourceInfo>> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
        let client = client.read().await;
        client.list_resources().await
    }

    /// Read a specific resource by URI from a specific MCP server.
    pub async fn read_resource(&self, server_name: &str, uri: &str) -> Result<McpResourceContent> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
        let client = client.read().await;
        client.read_resource(uri).await
    }

    // --- Prompts ---

    /// List available prompt templates from a specific MCP server.
    pub async fn list_prompts(&self, server_name: &str) -> Result<Vec<McpPromptInfo>> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
        let client = client.read().await;
        client.list_prompts().await
    }

    /// Get a specific prompt with arguments from a specific MCP server.
    pub async fn get_prompt(
        &self,
        server_name: &str,
        name: &str,
        args: HashMap<String, String>,
    ) -> Result<McpPromptResult> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
        let client = client.read().await;
        client.get_prompt(name, args).await
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_expand_env_vars_simple() {
        std::env::set_var("OCTO_TEST_VAR_XYZ", "hello");
        assert_eq!(expand_env_vars("${OCTO_TEST_VAR_XYZ}"), "hello");
        assert_eq!(expand_env_vars("pre-${OCTO_TEST_VAR_XYZ}-post"), "pre-hello-post");
        std::env::remove_var("OCTO_TEST_VAR_XYZ");
    }

    #[test]
    fn test_expand_env_vars_default() {
        std::env::remove_var("OCTO_TEST_MISSING_VAR");
        assert_eq!(expand_env_vars("${OCTO_TEST_MISSING_VAR:-fallback}"), "fallback");
        assert_eq!(expand_env_vars("${OCTO_TEST_MISSING_VAR}"), "");
    }

    #[test]
    fn test_expand_env_vars_no_expansion() {
        assert_eq!(expand_env_vars("no vars here"), "no vars here");
        assert_eq!(expand_env_vars("$plain"), "$plain");
    }

    #[test]
    fn test_load_config_octo_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            r#"{{ "servers": {{ "test-srv": {{ "command": "echo", "args": ["hello"], "env": {{}} }} }} }}"#
        )
        .unwrap();

        let configs = McpManager::load_config(tmp.path()).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "test-srv");
        assert_eq!(configs[0].command, "echo");
    }

    #[test]
    fn test_load_config_cc_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            r#"{{ "mcpServers": {{ "cc-srv": {{ "command": "npx", "args": ["-y", "@test/pkg"], "env": {{}} }} }} }}"#
        )
        .unwrap();

        let configs = McpManager::load_config(tmp.path()).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "cc-srv");
        assert_eq!(configs[0].command, "npx");
    }

    #[test]
    fn test_load_config_merged_format() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            r#"{{
                "servers": {{ "srv-a": {{ "command": "a", "args": [] }} }},
                "mcpServers": {{ "srv-b": {{ "command": "b", "args": [] }} }}
            }}"#
        )
        .unwrap();

        let configs = McpManager::load_config(tmp.path()).unwrap();
        assert_eq!(configs.len(), 2);
        let names: Vec<&str> = configs.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"srv-a"));
        assert!(names.contains(&"srv-b"));
    }

    #[test]
    fn test_load_config_env_expansion() {
        std::env::set_var("OCTO_TEST_API_KEY", "secret123");
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            r#"{{ "mcpServers": {{ "test": {{ "command": "cmd", "args": [], "env": {{ "API_KEY": "${{OCTO_TEST_API_KEY}}" }} }} }} }}"#
        )
        .unwrap();

        let configs = McpManager::load_config(tmp.path()).unwrap();
        assert_eq!(configs[0].env.get("API_KEY").unwrap(), "secret123");
        std::env::remove_var("OCTO_TEST_API_KEY");
    }

    #[test]
    fn test_add_to_config_file_creates_new() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("mcp.json");

        let config = McpServerConfig {
            name: "my-server".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/pkg".to_string()],
            env: HashMap::from([("KEY".to_string(), "val".to_string())]),
            auto_start: true,
        };

        McpManager::add_to_config_file(&config_path, &config).unwrap();
        assert!(config_path.exists());

        // Verify it loads back
        let loaded = McpManager::load_config(&config_path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "my-server");
        assert_eq!(loaded[0].command, "npx");
        assert_eq!(loaded[0].env.get("KEY").unwrap(), "val");
    }

    #[test]
    fn test_add_to_config_file_appends() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("mcp.json");

        let config1 = McpServerConfig {
            name: "srv-1".to_string(),
            command: "cmd1".to_string(),
            args: vec![],
            env: HashMap::new(),
            auto_start: true,
        };
        let config2 = McpServerConfig {
            name: "srv-2".to_string(),
            command: "cmd2".to_string(),
            args: vec![],
            env: HashMap::new(),
            auto_start: true,
        };

        McpManager::add_to_config_file(&config_path, &config1).unwrap();
        McpManager::add_to_config_file(&config_path, &config2).unwrap();

        let loaded = McpManager::load_config(&config_path).unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_remove_from_config_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("mcp.json");

        let config = McpServerConfig {
            name: "to-remove".to_string(),
            command: "cmd".to_string(),
            args: vec![],
            env: HashMap::new(),
            auto_start: true,
        };

        McpManager::add_to_config_file(&config_path, &config).unwrap();
        let removed = McpManager::remove_from_config_file(&config_path, "to-remove").unwrap();
        assert!(removed);

        let loaded = McpManager::load_config(&config_path).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_remove_from_config_file_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("nonexistent.json");
        let removed = McpManager::remove_from_config_file(&config_path, "x").unwrap();
        assert!(!removed);
    }
}
