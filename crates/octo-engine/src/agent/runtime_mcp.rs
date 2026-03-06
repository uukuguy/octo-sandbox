//! MCP server management methods for AgentRuntime.

use std::sync::Arc;

use octo_types::ToolSource;
use tokio::sync::RwLock;

use crate::mcp::stdio::StdioMcpClient;
use crate::mcp::traits::{McpClient, McpToolInfo};

use super::runtime::AgentRuntime;
use super::AgentError;

impl AgentRuntime {
    /// 添加 MCP Server → 自动注册 tools
    ///
    /// Connect and list_tools happen OUTSIDE the mcp_manager lock so that
    /// slow subprocess startup (e.g. npx downloading packages) does not
    /// block other handlers that need the lock for reads.
    pub async fn add_mcp_server(
        &self,
        config: crate::mcp::traits::McpServerConfig,
    ) -> Result<Vec<McpToolInfo>, AgentError> {
        let mcp = &self.mcp_manager;

        // Mark as starting (brief lock, no IO)
        {
            let mut guard = mcp.lock().await;
            guard.set_runtime_state(
                &config.name,
                crate::mcp::manager::ServerRuntimeState::Starting,
            );
        }

        // Connect and discover tools OUTSIDE the lock (may take seconds)
        let mut client = StdioMcpClient::new(config.clone());
        let connect_result = client.connect().await;
        if let Err(e) = connect_result {
            let mut guard = mcp.lock().await;
            guard.set_runtime_state(
                &config.name,
                crate::mcp::manager::ServerRuntimeState::Error {
                    message: e.to_string(),
                },
            );
            return Err(AgentError::McpError(e.to_string()));
        }

        let tools = match client.list_tools().await {
            Ok(t) => t,
            Err(e) => {
                let mut guard = mcp.lock().await;
                guard.set_runtime_state(
                    &config.name,
                    crate::mcp::manager::ServerRuntimeState::Error {
                        message: e.to_string(),
                    },
                );
                return Err(AgentError::McpError(e.to_string()));
            }
        };

        tracing::info!(
            server = %config.name,
            tool_count = tools.len(),
            "MCP server connected with tools"
        );

        let client: Arc<RwLock<Box<dyn McpClient>>> = Arc::new(RwLock::new(Box::new(client)));

        // Insert into manager and register tools (brief lock, no IO)
        {
            let mut guard = mcp.lock().await;
            guard.insert_connected_client(config.name.clone(), client.clone(), tools.clone());
        }

        // 注册到 ToolRegistry
        {
            let mut tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            for tool_info in &tools {
                let bridge = crate::mcp::bridge::McpToolBridge::new(
                    client.clone(),
                    config.name.clone(),
                    tool_info.clone(),
                );
                tools_guard.register(bridge);
            }
        }

        Ok(tools)
    }

    /// 移除 MCP Server → 自动注销 tools
    pub async fn remove_mcp_server(&self, name: &str) -> Result<(), AgentError> {
        let mcp = &self.mcp_manager;

        // 先获取要移除的 tools 信息
        let _removed_tool_names: Vec<String> = {
            let guard = mcp.lock().await;
            guard
                .get_tool_infos(name)
                .map(|tools| tools.iter().map(|t| t.name.clone()).collect())
                .unwrap_or_default()
        };

        // 调用 remove_server
        {
            let mut guard = mcp.lock().await;
            guard
                .remove_server(name)
                .await
                .map_err(|e| AgentError::McpError(e.to_string()))?;
        }

        // 从 ToolRegistry 注销
        // 由于 ToolRegistry 没有 unregister 方法，我们重新构建工具列表
        // 过滤掉属于该 MCP server 的工具
        let all_tools: Vec<(String, Arc<dyn crate::tools::Tool>)> = {
            let tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            tools_guard
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let mut new_registry = crate::tools::ToolRegistry::new();
        for (tool_name, tool) in all_tools {
            // 检查工具来源是否为该 MCP server，使用模式匹配
            let should_remove = match tool.source() {
                ToolSource::Mcp(server_name) => server_name == name,
                _ => false,
            };
            if should_remove {
                continue; // 跳过要移除的工具
            }
            new_registry.register_arc(tool_name, tool);
        }
        // 替换旧的 registry
        let mut tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        *tools_guard = new_registry;

        Ok(())
    }

    /// 列出运行中的 MCP servers
    pub async fn list_mcp_servers(&self) -> Vec<crate::mcp::manager::ServerRuntimeState> {
        let guard = self.mcp_manager.lock().await;
        let states = guard.all_runtime_states();
        states.into_iter().map(|(_, state)| state).collect()
    }

    /// 获取所有 MCP servers 的运行时状态（包含名称）
    pub async fn get_all_mcp_server_states(
        &self,
    ) -> std::collections::HashMap<String, crate::mcp::manager::ServerRuntimeState> {
        let guard = self.mcp_manager.lock().await;
        guard.all_runtime_states()
    }

    /// 获取指定 MCP server 的 tools
    pub async fn get_mcp_tool_infos(
        &self,
        server_id: &str,
    ) -> Vec<crate::mcp::traits::McpToolInfo> {
        let guard = self.mcp_manager.lock().await;
        guard.get_tool_infos(server_id).unwrap_or_default()
    }

    /// 调用 MCP tool
    ///
    /// The `Arc<RwLock<Box<dyn McpClient>>>` is cloned under a brief mutex lock,
    /// then the actual tool call (network I/O) happens OUTSIDE the lock so that
    /// concurrent calls are not serialized through the McpManager mutex.
    pub async fn call_mcp_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Brief lock: only to clone the client Arc, no I/O
        let client = {
            let guard = self.mcp_manager.lock().await;
            guard
                .clients()
                .get(server_id)
                .cloned()
                .ok_or_else(|| format!("MCP server not found: {server_id}"))?
        };
        // Network I/O outside the mutex
        let client_guard = client.read().await;
        client_guard
            .call_tool(tool_name, arguments)
            .await
            .map_err(|e| e.to_string())
    }

    /// 获取指定 MCP server 的运行时状态
    pub async fn get_mcp_runtime_state(
        &self,
        server_id: &str,
    ) -> crate::mcp::manager::ServerRuntimeState {
        let guard = self.mcp_manager.lock().await;
        guard.get_runtime_state(server_id)
    }

    /// 获取指定 MCP server 的 tool 数量
    pub async fn get_mcp_tool_count(&self, server_id: &str) -> usize {
        let guard = self.mcp_manager.lock().await;
        guard.get_tool_count(server_id)
    }
}
