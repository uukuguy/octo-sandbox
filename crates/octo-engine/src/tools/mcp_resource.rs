//! MCP Resource tools — list and read resources from connected MCP servers.
//!
//! Wraps McpManager::list_resources/read_resource as agent-callable tools.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::Mutex;

use super::traits::Tool;
use crate::mcp::manager::McpManager;

// ─── mcp_list_resources ────────────────────────────────────────────────────

pub struct McpListResourcesTool {
    manager: Arc<Mutex<McpManager>>,
}

impl McpListResourcesTool {
    pub fn new(manager: Arc<Mutex<McpManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for McpListResourcesTool {
    fn name(&self) -> &str {
        "mcp_list_resources"
    }

    fn description(&self) -> &str {
        "List available resources from connected MCP servers.\n\
         \n\
         ## Parameters\n\
         - server (required): Name of the MCP server to query\n\
         \n\
         ## Returns\n\
         Array of resources with uri, name, description, mime_type."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name to list resources from"
                }
            },
            "required": ["server"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let server = params["server"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: server"))?;

        let manager = self.manager.lock().await;
        match manager.list_resources(server).await {
            Ok(resources) => {
                let result = json!({
                    "server": server,
                    "resources": resources,
                    "count": resources.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to list resources from '{}': {}",
                server, e
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "mcp"
    }
}

// ─── mcp_read_resource ─────────────────────────────────────────────────────

pub struct McpReadResourceTool {
    manager: Arc<Mutex<McpManager>>,
}

impl McpReadResourceTool {
    pub fn new(manager: Arc<Mutex<McpManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for McpReadResourceTool {
    fn name(&self) -> &str {
        "mcp_read_resource"
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server by URI.\n\
         \n\
         ## Parameters\n\
         - server (required): Name of the MCP server\n\
         - uri (required): Resource URI to read\n\
         \n\
         ## Returns\n\
         Resource content (text or base64 blob with mime type)."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "Resource URI to read"
                }
            },
            "required": ["server", "uri"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let server = params["server"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: server"))?;
        let uri = params["uri"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: uri"))?;

        let manager = self.manager.lock().await;
        match manager.read_resource(server, uri).await {
            Ok(content) => {
                let result = json!({
                    "server": server,
                    "uri": content.uri,
                    "mime_type": content.mime_type,
                    "text": content.text,
                    "has_blob": content.blob.is_some(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to read resource '{}' from '{}': {}",
                uri, server, e
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "mcp"
    }
}
