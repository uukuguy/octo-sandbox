use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

use octo_types::{RiskLevel, ToolContext, ToolResult, ToolSource};

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

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let client = self.client.read().await;
        match client.call_tool(&self.tool_info.name, params).await {
            Ok(result) => {
                let is_error = result
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let content = result
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if is_error {
                    Ok(ToolResult::error(content))
                } else {
                    Ok(ToolResult::success(content))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("MCP tool error: {e}"))),
        }
    }
}
