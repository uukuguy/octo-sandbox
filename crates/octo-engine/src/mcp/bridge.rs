use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::{McpClient, McpToolInfo};
use crate::tools::Tool;

/// Bridges an MCP server tool into the local ToolRegistry.
pub struct McpToolBridge {
    client: Arc<RwLock<Box<dyn McpClient>>>,
    server_name: String,
    tool_info: McpToolInfo,
}

impl McpToolBridge {
    pub fn new(
        client: Arc<RwLock<Box<dyn McpClient>>>,
        server_name: String,
        tool_info: McpToolInfo,
    ) -> Self {
        Self {
            client,
            server_name,
            tool_info,
        }
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn name(&self) -> &str {
        &self.tool_info.name
    }

    fn description(&self) -> &str {
        self.tool_info.description.as_deref().unwrap_or("")
    }

    fn parameters(&self) -> serde_json::Value {
        self.tool_info.input_schema.clone()
    }

    fn source(&self) -> ToolSource {
        ToolSource::Mcp(self.server_name.clone())
    }

    fn risk_level(&self) -> RiskLevel {
        match &self.tool_info.annotations {
            Some(ann) if ann.destructive => RiskLevel::Destructive,
            Some(ann) if ann.open_world => RiskLevel::HighRisk,
            Some(ann) if ann.read_only => RiskLevel::ReadOnly,
            _ => RiskLevel::LowRisk,
        }
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // Catch panics from rmcp transport layer (e.g., broken pipe when server process dies)
        let client = self.client.clone();
        let tool_name = self.tool_info.name.clone();
        let server_name = self.server_name.clone();

        let result = tokio::task::spawn(async move {
            let client_guard = client.read().await;
            client_guard.call_tool(&tool_name, params).await
        })
        .await;

        match result {
            Ok(Ok(value)) => {
                let is_error = value
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let content = value
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if is_error {
                    Ok(ToolOutput::error(content))
                } else {
                    Ok(ToolOutput::success(content))
                }
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!(
                "MCP tool error (server '{}'): {e}",
                server_name
            ))),
            Err(join_err) => {
                tracing::error!(
                    server = %server_name,
                    error = %join_err,
                    "MCP tool call panicked — server process may have crashed"
                );
                Ok(ToolOutput::error(format!(
                    "MCP server '{}' crashed during tool call: {join_err}",
                    server_name
                )))
            }
        }
    }
}
