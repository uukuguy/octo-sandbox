//! MCP Server role — expose ToolRegistry tools to external MCP clients.
//!
//! This module implements `rmcp::ServerHandler` so that octo-engine can act as
//! an MCP **server**, allowing external agents or tools to discover and invoke
//! the tools registered in a `ToolRegistry`.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, InitializeResult,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo,
    ToolAnnotations as RmcpToolAnnotations, ToolsCapability,
};
use rmcp::model::{ErrorData as McpError, JsonObject};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde_json::Value;

use octo_types::{RiskLevel, SandboxId, ToolContext};

use crate::tools::{Tool, ToolRegistry};

/// Configuration for the OctoMcpServer (distinct from `McpServerConfig` in traits.rs
/// which configures MCP *client* connections to external servers).
#[derive(Debug, Clone)]
pub struct OctoMcpServerConfig {
    /// Server name reported to clients.
    pub name: String,
    /// Server version reported to clients.
    pub version: String,
    /// Optional human-readable instructions for clients.
    pub instructions: Option<String>,
}

impl Default for OctoMcpServerConfig {
    fn default() -> Self {
        Self {
            name: "octo-engine".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            instructions: None,
        }
    }
}

/// MCP Server that exposes `ToolRegistry` tools to external MCP clients.
///
/// Implements `rmcp::ServerHandler` to handle `tools/list` and `tools/call`
/// requests from MCP clients connected via stdio or HTTP transport.
pub struct OctoMcpServer {
    registry: Arc<ToolRegistry>,
    config: OctoMcpServerConfig,
    tool_context: ToolContext,
}

impl OctoMcpServer {
    pub fn new(registry: Arc<ToolRegistry>, config: OctoMcpServerConfig) -> Self {
        let tool_context = ToolContext {
            sandbox_id: SandboxId::new(),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        Self {
            registry,
            config,
            tool_context,
        }
    }

    /// Create with a custom ToolContext for tool execution.
    pub fn with_tool_context(mut self, ctx: ToolContext) -> Self {
        self.tool_context = ctx;
        self
    }

    /// Convert an internal `Tool` trait object into an rmcp `Tool`.
    fn to_rmcp_tool(tool: &dyn Tool) -> rmcp::model::Tool {
        let spec = tool.spec();

        let input_schema: JsonObject = match spec.input_schema {
            Value::Object(map) => map,
            _ => {
                let mut map = serde_json::Map::new();
                map.insert("type".to_string(), Value::String("object".to_string()));
                map
            }
        };

        let annotations = match tool.risk_level() {
            RiskLevel::ReadOnly => Some(
                RmcpToolAnnotations::new()
                    .read_only(true)
                    .destructive(false),
            ),
            RiskLevel::Destructive => Some(
                RmcpToolAnnotations::new()
                    .read_only(false)
                    .destructive(true),
            ),
            RiskLevel::HighRisk => Some(
                RmcpToolAnnotations::new()
                    .read_only(false)
                    .open_world(true),
            ),
            _ => None,
        };

        let mut rmcp_tool = rmcp::model::Tool::new(
            Cow::Owned(spec.name),
            Cow::Owned(spec.description),
            Arc::new(input_schema),
        );
        if let Some(ann) = annotations {
            rmcp_tool = rmcp_tool.with_annotations(ann);
        }
        rmcp_tool
    }
}

impl ServerHandler for OctoMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut caps = ServerCapabilities::default();
        caps.tools = Some(ToolsCapability::default());

        InitializeResult::new(caps)
            .with_server_info(Implementation::new(
                self.config.name.clone(),
                self.config.version.clone(),
            ))
            .with_instructions(
                self.config
                    .instructions
                    .clone()
                    .unwrap_or_default(),
            )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let tools: Vec<rmcp::model::Tool> = self
            .registry
            .iter()
            .map(|(_name, tool)| Self::to_rmcp_tool(tool.as_ref()))
            .collect();

        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let name = request.name.to_string();
        let arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or(Value::Object(serde_json::Map::new()));

        async move {
            let tool = self.registry.get(&name).ok_or_else(|| {
                McpError::invalid_params(format!("Unknown tool: {name}"), None)
            })?;

            match tool.execute(arguments, &self.tool_context).await {
                Ok(output) => {
                    let content = vec![Content::text(output.content)];
                    if output.is_error {
                        Ok(CallToolResult::error(content))
                    } else {
                        Ok(CallToolResult::success(content))
                    }
                }
                Err(e) => {
                    let content = vec![Content::text(format!("Tool execution error: {e}"))];
                    Ok(CallToolResult::error(content))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use octo_types::{ToolOutput, ToolSource};

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input back"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message to echo" }
                },
                "required": ["message"]
            })
        }
        async fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            let msg = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(empty)");
            Ok(ToolOutput::success(msg))
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
    }

    struct FailTool;

    #[async_trait]
    impl Tool for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            anyhow::bail!("intentional failure")
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
    }

    struct ErrorOutputTool;

    #[async_trait]
    impl Tool for ErrorOutputTool {
        fn name(&self) -> &str {
            "error_output"
        }
        fn description(&self) -> &str {
            "Returns error output"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::error("something went wrong"))
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
    }

    struct ReadOnlyTool;

    #[async_trait]
    impl Tool for ReadOnlyTool {
        fn name(&self) -> &str {
            "readonly"
        }
        fn description(&self) -> &str {
            "A read-only tool"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::success("ok"))
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::ReadOnly
        }
    }

    fn make_registry() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool);
        reg.register(FailTool);
        reg.register(ErrorOutputTool);
        reg.register(ReadOnlyTool);
        Arc::new(reg)
    }

    fn make_server() -> OctoMcpServer {
        OctoMcpServer::new(make_registry(), OctoMcpServerConfig::default())
    }

    #[test]
    fn test_mcp_server_capabilities() {
        let server = make_server();
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
        assert_eq!(info.server_info.name, "octo-engine");
        assert!(!info.server_info.version.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_server_list_tools() {
        let server = make_server();
        let tools = server
            .registry
            .iter()
            .map(|(_name, tool)| OctoMcpServer::to_rmcp_tool(tool.as_ref()))
            .collect::<Vec<_>>();

        assert_eq!(tools.len(), 4);
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.contains(&"echo".to_string()));
        assert!(names.contains(&"fail".to_string()));
        assert!(names.contains(&"error_output".to_string()));
        assert!(names.contains(&"readonly".to_string()));
    }

    #[tokio::test]
    async fn test_mcp_server_call_tool() {
        let server = make_server();
        let tool = server.registry.get("echo").expect("echo tool exists");
        let result = tool
            .execute(
                serde_json::json!({"message": "hello world"}),
                &server.tool_context,
            )
            .await
            .expect("should succeed");
        assert!(!result.is_error);
        assert_eq!(result.content, "hello world");

        let rmcp_tool = OctoMcpServer::to_rmcp_tool(tool.as_ref());
        assert_eq!(rmcp_tool.name.as_ref(), "echo");
        assert_eq!(
            rmcp_tool.description.as_deref(),
            Some("Echoes the input back")
        );
    }

    #[tokio::test]
    async fn test_mcp_server_unknown_tool() {
        let server = make_server();
        let tool = server.registry.get("nonexistent");
        assert!(tool.is_none());
    }

    #[tokio::test]
    async fn test_mcp_server_tool_error() {
        let server = make_server();
        let tool = server.registry.get("fail").expect("fail tool exists");
        let result = tool
            .execute(serde_json::json!({}), &server.tool_context)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("intentional failure"));
    }

    #[test]
    fn test_mcp_server_tool_annotations() {
        let server = make_server();
        let tool = server.registry.get("readonly").expect("readonly tool exists");
        let rmcp_tool = OctoMcpServer::to_rmcp_tool(tool.as_ref());
        let ann = rmcp_tool.annotations.expect("should have annotations");
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
    }

    #[tokio::test]
    async fn test_mcp_server_error_output_tool() {
        let server = make_server();
        let tool = server
            .registry
            .get("error_output")
            .expect("error_output tool exists");
        let result = tool
            .execute(serde_json::json!({}), &server.tool_context)
            .await
            .expect("should not throw");
        assert!(result.is_error);
        assert_eq!(result.content, "something went wrong");
    }
}
