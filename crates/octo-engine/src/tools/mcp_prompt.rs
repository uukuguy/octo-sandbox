//! MCP Prompt tools — list and execute prompts from connected MCP servers.
//!
//! Wraps McpManager::list_prompts/get_prompt as agent-callable tools.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::Mutex;

use super::traits::Tool;
use crate::mcp::manager::McpManager;

// ─── mcp_list_prompts ──────────────────────────────────────────────────────

pub struct McpListPromptsTool {
    manager: Arc<Mutex<McpManager>>,
}

impl McpListPromptsTool {
    pub fn new(manager: Arc<Mutex<McpManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for McpListPromptsTool {
    fn name(&self) -> &str {
        "mcp_list_prompts"
    }

    fn description(&self) -> &str {
        "List available prompt templates from an MCP server.\n\
         \n\
         ## Parameters\n\
         - server (required): Name of the MCP server to query\n\
         \n\
         ## Returns\n\
         Array of prompts with name, description, and accepted arguments."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name to list prompts from"
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
        match manager.list_prompts(server).await {
            Ok(prompts) => {
                let result = json!({
                    "server": server,
                    "prompts": prompts,
                    "count": prompts.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to list prompts from '{}': {}",
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

// ─── mcp_get_prompt ────────────────────────────────────────────────────────

pub struct McpGetPromptTool {
    manager: Arc<Mutex<McpManager>>,
}

impl McpGetPromptTool {
    pub fn new(manager: Arc<Mutex<McpManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for McpGetPromptTool {
    fn name(&self) -> &str {
        "mcp_get_prompt"
    }

    fn description(&self) -> &str {
        "Execute a prompt template from an MCP server with arguments.\n\
         \n\
         ## Parameters\n\
         - server (required): Name of the MCP server\n\
         - name (required): Prompt template name\n\
         - arguments (optional): Key-value arguments for the prompt\n\
         \n\
         ## Returns\n\
         Rendered prompt messages (role + content pairs)."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name"
                },
                "name": {
                    "type": "string",
                    "description": "Prompt template name"
                },
                "arguments": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "Key-value arguments for the prompt"
                }
            },
            "required": ["server", "name"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let server = params["server"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: server"))?;
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

        let arguments: HashMap<String, String> = params
            .get("arguments")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let manager = self.manager.lock().await;
        match manager.get_prompt(server, name, arguments).await {
            Ok(result) => {
                let output = json!({
                    "server": server,
                    "prompt": name,
                    "description": result.description,
                    "messages": result.messages,
                    "message_count": result.messages.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&output)?))
            }
            Err(e) => Ok(ToolOutput::error(format!(
                "Failed to get prompt '{}' from '{}': {}",
                name, server, e
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
