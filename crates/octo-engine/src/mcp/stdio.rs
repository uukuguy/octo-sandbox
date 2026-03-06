use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{debug, info, warn};

use rmcp::model::{
    CallToolRequestParams, GetPromptRequestParams, RawContent, ReadResourceRequestParams,
};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};

use super::traits::{
    McpClient, McpPromptArgument, McpPromptInfo, McpPromptMessage, McpPromptResult,
    McpResourceContent, McpResourceInfo, McpServerConfig, McpToolInfo,
};

pub struct StdioMcpClient {
    config: McpServerConfig,
    service: Option<RunningService<RoleClient, ()>>,
}

impl StdioMcpClient {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            service: None,
        }
    }
}

#[async_trait]
impl McpClient for StdioMcpClient {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn connect(&mut self) -> Result<()> {
        let config = &self.config;
        info!(
            name = %config.name,
            command = %config.command,
            "Connecting to MCP server"
        );

        let env = config.env.clone();
        let args = config.args.clone();

        let transport = TokioChildProcess::new(
            tokio::process::Command::new(&config.command).configure(move |c| {
                for arg in &args {
                    c.arg(arg);
                }
                for (k, v) in &env {
                    c.env(k, v);
                }
            }),
        )
        .context("Failed to spawn MCP server process")?;

        let service = ()
            .serve(transport)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize MCP connection: {e}"))?;

        let peer_info = service.peer_info();
        info!(
            name = %config.name,
            server = ?peer_info,
            "MCP server connected"
        );

        self.service = Some(service);
        Ok(())
    }

    async fn list_tools(&self) -> Result<Vec<McpToolInfo>> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let tools = service
            .list_all_tools()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list MCP tools: {e}"))?;

        let result: Vec<McpToolInfo> = tools
            .into_iter()
            .map(|t| McpToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()),
                input_schema: serde_json::Value::Object(t.input_schema.as_ref().clone()),
            })
            .collect();

        debug!(count = result.len(), "Listed MCP tools");
        Ok(result)
    }

    async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let arguments = args.as_object().map(|o| o.clone());

        let result = service
            .call_tool(CallToolRequestParams {
                meta: None,
                name: name.to_string().into(),
                arguments,
                task: None,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to call MCP tool '{name}': {e}"))?;

        // Convert result content to JSON
        let content_strs: Vec<String> = result
            .content
            .into_iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect();

        Ok(serde_json::json!({
            "content": content_strs.join("\n"),
            "isError": result.is_error.unwrap_or(false),
        }))
    }

    fn is_connected(&self) -> bool {
        self.service.is_some()
    }

    async fn shutdown(&mut self) -> Result<()> {
        if let Some(service) = self.service.take() {
            info!(name = %self.config.name, "Shutting down MCP server");
            service
                .cancel()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to cancel MCP service: {e}"))?;
        }
        Ok(())
    }

    async fn list_resources(&self) -> Result<Vec<McpResourceInfo>> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let resources = match service.list_all_resources().await {
            Ok(r) => r,
            Err(e) => {
                warn!(name = %self.config.name, error = %e, "Server does not support resources or list failed");
                return Ok(vec![]);
            }
        };

        let result: Vec<McpResourceInfo> = resources
            .into_iter()
            .map(|r| McpResourceInfo {
                uri: r.uri.clone(),
                name: r.name.clone(),
                description: r.description.clone(),
                mime_type: r.mime_type.clone(),
            })
            .collect();

        debug!(count = result.len(), "Listed MCP resources");
        Ok(result)
    }

    async fn read_resource(&self, uri: &str) -> Result<McpResourceContent> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let result = service
            .read_resource(ReadResourceRequestParams {
                meta: None,
                uri: uri.to_string(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read MCP resource '{uri}': {e}"))?;

        // Take the first content entry (most common case)
        let content = result.contents.into_iter().next();
        match content {
            Some(rmcp::model::ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                ..
            }) => Ok(McpResourceContent {
                uri,
                mime_type,
                text: Some(text),
                blob: None,
            }),
            Some(rmcp::model::ResourceContents::BlobResourceContents {
                uri,
                mime_type,
                blob,
                ..
            }) => Ok(McpResourceContent {
                uri,
                mime_type,
                text: None,
                blob: Some(blob),
            }),
            None => Ok(McpResourceContent {
                uri: uri.to_string(),
                mime_type: None,
                text: None,
                blob: None,
            }),
        }
    }

    async fn list_prompts(&self) -> Result<Vec<McpPromptInfo>> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let prompts = match service.list_all_prompts().await {
            Ok(p) => p,
            Err(e) => {
                warn!(name = %self.config.name, error = %e, "Server does not support prompts or list failed");
                return Ok(vec![]);
            }
        };

        let result: Vec<McpPromptInfo> = prompts
            .into_iter()
            .map(|p| McpPromptInfo {
                name: p.name.clone(),
                description: p.description.clone(),
                arguments: p
                    .arguments
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| McpPromptArgument {
                        name: a.name,
                        description: a.description,
                        required: a.required.unwrap_or(false),
                    })
                    .collect(),
            })
            .collect();

        debug!(count = result.len(), "Listed MCP prompts");
        Ok(result)
    }

    async fn get_prompt(
        &self,
        name: &str,
        args: HashMap<String, String>,
    ) -> Result<McpPromptResult> {
        let service = self
            .service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let arguments: Option<serde_json::Map<String, serde_json::Value>> = if args.is_empty() {
            None
        } else {
            Some(
                args.into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect(),
            )
        };

        let result = service
            .get_prompt(GetPromptRequestParams {
                meta: None,
                name: name.to_string(),
                arguments,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get MCP prompt '{name}': {e}"))?;

        Ok(McpPromptResult {
            description: result.description,
            messages: result
                .messages
                .into_iter()
                .map(|m| {
                    let role = match m.role {
                        rmcp::model::PromptMessageRole::User => "user".to_string(),
                        rmcp::model::PromptMessageRole::Assistant => "assistant".to_string(),
                    };
                    let content = match m.content {
                        rmcp::model::PromptMessageContent::Text { text } => text,
                        other => serde_json::to_string(&other).unwrap_or_default(),
                    };
                    McpPromptMessage { role, content }
                })
                .collect(),
        })
    }
}
