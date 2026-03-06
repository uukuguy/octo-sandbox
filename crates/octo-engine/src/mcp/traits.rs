use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// MCP 服务器传输方式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// 本地进程 stdin/stdout（默认）
    #[default]
    Stdio,
    /// Streamable HTTP / SSE（远程服务器）
    Sse,
}

impl std::str::FromStr for McpTransport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdio" => Ok(McpTransport::Stdio),
            "sse" | "http" | "streamable-http" => Ok(McpTransport::Sse),
            other => Err(format!(
                "Unknown MCP transport: '{other}'. Expected 'stdio' or 'sse'"
            )),
        }
    }
}

/// Info about a tool provided by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

/// MCP Resource information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// Content of a read resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    /// Base64-encoded binary content.
    pub blob: Option<String>,
}

/// MCP Prompt template information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptInfo {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgument>,
}

/// A single argument accepted by an MCP prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

/// Result of getting a prompt with arguments filled in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptResult {
    pub description: Option<String>,
    pub messages: Vec<McpPromptMessage>,
}

/// A single message in a prompt result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptMessage {
    /// "user" or "assistant"
    pub role: String,
    pub content: String,
}

/// Configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Configuration for an MCP server (persisted version with ID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigV2 {
    pub id: String,
    pub name: String,
    pub source: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub enabled: bool,
    #[serde(default)]
    pub transport: McpTransport,
    /// SSE transport 专用：服务器 URL（如 "http://localhost:8080/mcp"）
    #[serde(default)]
    pub url: Option<String>,
}

impl From<McpServerConfigV2> for McpServerConfig {
    fn from(v2: McpServerConfigV2) -> Self {
        Self {
            name: v2.name,
            command: v2.command,
            args: v2.args,
            env: v2.env,
        }
    }
}

/// Abstraction over MCP protocol client.
#[async_trait]
pub trait McpClient: Send + Sync {
    /// Server name.
    fn name(&self) -> &str;

    /// Connect to the MCP server (spawn process + handshake).
    async fn connect(&mut self) -> Result<()>;

    /// List tools provided by the server.
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>>;

    /// Call a tool on the server.
    async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value>;

    /// Check if connected.
    fn is_connected(&self) -> bool;

    /// Graceful shutdown.
    async fn shutdown(&mut self) -> Result<()>;

    // --- Resources ---

    /// List available resources from the MCP server.
    /// Default returns empty vec for servers that do not support resources.
    async fn list_resources(&self) -> Result<Vec<McpResourceInfo>> {
        Ok(vec![])
    }

    /// Read a specific resource by URI.
    /// Default returns an error for servers that do not support resources.
    async fn read_resource(&self, uri: &str) -> Result<McpResourceContent> {
        Err(anyhow::anyhow!(
            "read_resource not supported by this MCP client (uri: {uri})"
        ))
    }

    // --- Prompts ---

    /// List available prompt templates from the MCP server.
    /// Default returns empty vec for servers that do not support prompts.
    async fn list_prompts(&self) -> Result<Vec<McpPromptInfo>> {
        Ok(vec![])
    }

    /// Get a specific prompt with arguments filled in.
    /// Default returns an error for servers that do not support prompts.
    async fn get_prompt(
        &self,
        name: &str,
        args: HashMap<String, String>,
    ) -> Result<McpPromptResult> {
        let _ = args;
        Err(anyhow::anyhow!(
            "get_prompt not supported by this MCP client (prompt: {name})"
        ))
    }
}
