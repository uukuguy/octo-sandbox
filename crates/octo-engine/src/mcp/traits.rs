use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};

/// Validates a resource URI for MCP read_resource calls (MCP-01).
///
/// Allowed schemes: `mcp://`, `resource://`, `file://` (local paths only).
/// Rejected: `http://` or `https://` pointing to private/loopback IP ranges,
/// any URI containing path traversal sequences (`..`).
pub fn validate_resource_uri(uri: &str) -> anyhow::Result<()> {
    // Reject path traversal in the raw URI string before any parsing.
    if uri.contains("..") {
        return Err(anyhow::anyhow!(
            "Invalid resource URI: path traversal detected in '{uri}'"
        ));
    }

    // Determine scheme (everything before the first ':').
    let scheme = uri
        .split_once(':')
        .map(|(s, _)| s.to_lowercase())
        .unwrap_or_default();

    match scheme.as_str() {
        "mcp" | "resource" => Ok(()),
        "file" => {
            // file:// is only allowed for local paths (no host, or host == "localhost").
            // A well-formed local file URI looks like file:///path or file://localhost/path.
            let path_part = uri.strip_prefix("file://").unwrap_or("");
            let host = path_part.split('/').next().unwrap_or("");
            if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Invalid resource URI: file:// URIs must be local (got '{uri}')"
                ))
            }
        }
        "http" | "https" => {
            // Parse the host and reject private/loopback ranges.
            reject_private_host_in_uri(uri).map_err(|_| {
                anyhow::anyhow!(
                    "Invalid resource URI: '{uri}' targets a private or loopback address"
                )
            })
        }
        _ => Err(anyhow::anyhow!(
            "Invalid resource URI: unsupported scheme '{scheme}' in '{uri}'"
        )),
    }
}

/// Validates an MCP server URL before creating an SSE transport (MCP-02).
///
/// Rules:
/// - `https://` is always allowed for public hosts.
/// - `http://` is only allowed for `localhost` / `127.0.0.1` (dev mode).
/// - Private IP ranges are rejected for both schemes.
pub fn validate_server_url(url: &str) -> anyhow::Result<()> {
    let parsed = Url::parse(url).map_err(|e| anyhow::anyhow!("Invalid server URL '{url}': {e}"))?;

    let scheme = parsed.scheme();
    let host_str = parsed.host_str().unwrap_or("");

    match scheme {
        "https" => {
            if is_private_host(host_str) {
                return Err(anyhow::anyhow!(
                    "Invalid server URL: '{url}' targets a private IP address"
                ));
            }
            Ok(())
        }
        "http" => {
            // HTTP is only allowed for loopback (dev/local use).
            if host_str == "localhost" || host_str == "127.0.0.1" || host_str == "::1" {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Invalid server URL: plain http:// is only allowed for localhost, got '{url}'"
                ))
            }
        }
        other => Err(anyhow::anyhow!(
            "Invalid server URL: unsupported scheme '{other}' in '{url}'"
        )),
    }
}

/// Returns `true` if the given host string resolves to a private or loopback IP range.
///
/// Covered ranges:
/// - Loopback: `127.0.0.0/8`, `::1`
/// - Link-local: `169.254.0.0/16`
/// - RFC-1918 private: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
fn is_private_host(host: &str) -> bool {
    // Try to parse as a raw IP first.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }
    // Named hosts: only "localhost" / "::1" shorthand — everything else is assumed public.
    matches!(host, "localhost")
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // 127.x.x.x
            o[0] == 127
            // 10.x.x.x
            || o[0] == 10
            // 172.16.x.x – 172.31.x.x
            || (o[0] == 172 && (16..=31).contains(&o[1]))
            // 192.168.x.x
            || (o[0] == 192 && o[1] == 168)
            // 169.254.x.x (link-local)
            || (o[0] == 169 && o[1] == 254)
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Helper: parse a URI and reject it when its host is in a private range.
fn reject_private_host_in_uri(uri: &str) -> anyhow::Result<()> {
    let parsed =
        Url::parse(uri).map_err(|e| anyhow::anyhow!("Failed to parse URI '{uri}': {e}"))?;
    let host = parsed.host_str().unwrap_or("");
    if is_private_host(host) {
        return Err(anyhow::anyhow!("private host"));
    }
    Ok(())
}

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

/// MCP tool annotations per MCP 2025-03 specification
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpToolAnnotations {
    /// Whether the tool only reads data (no side effects)
    #[serde(default)]
    pub read_only: bool,
    /// Whether the tool can cause destructive/irreversible changes
    #[serde(default)]
    pub destructive: bool,
    /// Whether the tool has side effects visible outside the session
    #[serde(default)]
    pub open_world: bool,
    /// Human-readable title
    pub title: Option<String>,
}

/// Info about a tool provided by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    /// MCP tool annotations (read-only, destructive, etc.)
    #[serde(default)]
    pub annotations: Option<McpToolAnnotations>,
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
    /// Whether to auto-start this server on runtime initialization.
    /// Defaults to `true`. Set to `false` to defer startup until explicit request.
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

fn default_auto_start() -> bool {
    true
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
    /// OAuth 2.1 configuration for SSE transport authentication.
    #[serde(default)]
    pub oauth: Option<super::oauth::OAuthConfig>,
}

impl From<McpServerConfigV2> for McpServerConfig {
    fn from(v2: McpServerConfigV2) -> Self {
        Self {
            name: v2.name,
            command: v2.command,
            args: v2.args,
            env: v2.env,
            auto_start: true,
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
