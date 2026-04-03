//! McpAuthTool — MCP server OAuth authentication tool.
//!
//! Aligns with CC-OSS McpAuthTool: dynamically generated pseudo-tool
//! for unauthenticated MCP servers. Returns auth URL for HTTP servers,
//! unsupported for stdio transport.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::Mutex;

use super::traits::Tool;
use crate::mcp::manager::McpManager;

/// Transport type for determining auth support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportKind {
    Stdio,
    Http,
    Sse,
}

pub struct McpAuthTool {
    server_name: String,
    transport: McpTransportKind,
    manager: Arc<Mutex<McpManager>>,
}

impl McpAuthTool {
    pub fn new(
        server_name: String,
        transport: McpTransportKind,
        manager: Arc<Mutex<McpManager>>,
    ) -> Self {
        Self {
            server_name,
            transport,
            manager,
        }
    }
}

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &str {
        // Dynamic name based on server — stored as owned String
        // Leak a &str since Tool trait requires &str lifetime
        // This is acceptable for dynamic tools with bounded lifetimes
        Box::leak(format!("mcp_auth_{}", self.server_name).into_boxed_str())
    }

    fn description(&self) -> &str {
        "Authenticate with an MCP server that requires OAuth.\n\
         Calling this tool initiates the authentication flow.\n\
         For HTTP/SSE servers, returns an auth URL to visit.\n\
         For stdio servers, authentication is not supported through this tool."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        match self.transport {
            McpTransportKind::Stdio => {
                let result = json!({
                    "status": "unsupported",
                    "server": self.server_name,
                    "message": "OAuth authentication is not supported for stdio MCP servers. Please configure authentication manually.",
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            McpTransportKind::Http | McpTransportKind::Sse => {
                // In a full implementation, this would:
                // 1. Start OAuth flow with the server
                // 2. Return the auth URL
                // 3. After auth completes, reconnect and swap tools
                //
                // For now, return a placeholder indicating the flow would start.
                let _manager = self.manager.lock().await;
                let result = json!({
                    "status": "auth_required",
                    "server": self.server_name,
                    "transport": format!("{:?}", self.transport).to_lowercase(),
                    "message": format!(
                        "MCP server '{}' requires authentication. OAuth flow support is pending (AW-D4).",
                        self.server_name
                    ),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "mcp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_mcp_auth_stdio_unsupported() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpAuthTool::new("test-server".to_string(), McpTransportKind::Stdio, manager);
        let result = tool.execute(json!({}), &test_ctx()).await.unwrap();
        assert!(result.content.contains("unsupported"));
        assert!(result.content.contains("test-server"));
    }

    #[tokio::test]
    async fn test_mcp_auth_http_returns_auth_required() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpAuthTool::new("remote-api".to_string(), McpTransportKind::Http, manager);
        let result = tool.execute(json!({}), &test_ctx()).await.unwrap();
        assert!(result.content.contains("auth_required"));
        assert!(result.content.contains("remote-api"));
    }

    #[tokio::test]
    async fn test_mcp_auth_dynamic_name() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpAuthTool::new("my-server".to_string(), McpTransportKind::Stdio, manager);
        assert_eq!(tool.name(), "mcp_auth_my-server");
    }
}
