use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::info;

use octo_types::{
    ChatMessage, ContentBlock, MessageRole, SandboxId, SessionId, TenantId, ToolContext, UserId,
};

use crate::agent::{
    AgentCatalog, AgentConfig, AgentError, AgentEvent, AgentExecutor, AgentExecutorHandle, AgentId,
    AgentLoop, AgentManifest, AgentMessage, AgentStatus, CancellationToken, TenantContext,
};
use crate::db::Database;
use crate::event::EventBus;
use crate::mcp::manager::McpManager;
use crate::mcp::traits::McpToolInfo;
use crate::memory::store_traits::MemoryStore;
use crate::memory::{InMemoryWorkingMemory, SqliteMemoryStore, SqliteWorkingMemory, WorkingMemory};
use crate::metering::Metering;
use crate::providers::ProviderConfig;
use crate::providers::{create_provider, Provider, ProviderChain, ProviderChainConfig};
use crate::scheduler::ScheduledTask;
use crate::session::{SessionStore, SqliteSessionStore};
use crate::skills::{SkillLoader, SkillRegistry, SkillTool};
use crate::tools::recorder::ToolExecutionRecorder;
use crate::tools::{default_tools, register_memory_tools, ToolRegistry};

const MPSC_CAPACITY: usize = 32;
const BROADCAST_CAPACITY: usize = 256;

/// AgentRuntime configuration - a subset of server Config needed by AgentRuntime
#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    /// Database path for SQLite storage
    pub db_path: String,
    /// LLM provider configuration
    pub provider: ProviderConfig,
    /// Skills directories to load from
    pub skills_dirs: Vec<String>,
    /// Provider chain configuration (optional)
    pub provider_chain: Option<ProviderChainConfig>,
    /// Working directory for sandbox (optional)
    pub working_dir: Option<PathBuf>,
    /// Enable event bus for observability
    pub enable_event_bus: bool,
}

impl AgentRuntimeConfig {
    /// Create from full server Config fields
    pub fn from_parts(
        db_path: String,
        provider: ProviderConfig,
        skills_dirs: Vec<String>,
        provider_chain: Option<ProviderChainConfig>,
        working_dir: Option<PathBuf>,
        enable_event_bus: bool,
    ) -> Self {
        Self {
            db_path,
            provider,
            skills_dirs,
            provider_chain,
            working_dir,
            enable_event_bus,
        }
    }
}

/// Session → AgentExecutorHandle 的注册表，同时持有所有共享运行时依赖
pub struct AgentRuntime {
    /// 单一主 executor（单用户场景）- 使用 Mutex 实现内部可变性
    primary_handle: Mutex<Option<AgentExecutorHandle>>,
    /// AgentId → CancellationToken，用于 stop/pause 时取消正在运行的 AgentExecutor
    agent_handles: DashMap<AgentId, CancellationToken>,
    // 定义层
    catalog: Arc<AgentCatalog>,
    // 共享依赖（构造时注入一次）
    provider: Arc<dyn Provider>,
    tools: Arc<StdMutex<ToolRegistry>>,
    skill_registry: Option<Arc<SkillRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Arc<dyn MemoryStore>,
    session_store: Arc<dyn SessionStore>,
    default_model: String,
    // Observability: event bus already forwarded to AgentExecutor at line 482
    event_bus: Option<Arc<EventBus>>,
    recorder: Arc<ToolExecutionRecorder>,
    provider_chain: Option<Arc<ProviderChain>>,
    // Runtime fields (Task 2)
    mcp_manager: Arc<Mutex<crate::mcp::manager::McpManager>>,
    working_dir: PathBuf,
    // Observability: metering for token usage tracking
    metering: Arc<Metering>,
    // Tenant isolation (Task 3)
    tenant_context: Option<TenantContext>,
}

impl AgentRuntime {
    /// Create a new AgentRuntime with all components internalized.
    ///
    /// # Arguments
    /// * `catalog` - Agent catalog (created externally with store)
    /// * `config` - Runtime configuration containing db_path, provider, skills, etc.
    /// * `tenant_context` - Optional tenant context for multi-tenant isolation.
    ///                      Pass `None` for single-user mode (octo-workbench).
    ///
    /// # Returns
    /// A fully initialized AgentRuntime with:
    /// - Database connection (from db_path)
    /// - WorkingMemory (SqliteWorkingMemory)
    /// - SessionStore (SqliteSessionStore)
    /// - MemoryStore (SqliteMemoryStore)
    /// - ToolExecutionRecorder
    /// - ToolRegistry (default + memory + skills)
    /// - SkillRegistry (loaded from config.skills_dirs)
    /// - Provider (from config.provider)
    /// - ProviderChain (if configured)
    pub async fn new(
        catalog: Arc<AgentCatalog>,
        config: AgentRuntimeConfig,
        tenant_context: Option<TenantContext>,
    ) -> Result<Self, AgentError> {
        // 1. Open database
        let db = Database::open(&config.db_path)
            .await
            .map_err(|e| AgentError::Internal(format!("Failed to open database: {}", e)))?;
        let conn = db.conn().clone();

        // 2. Create WorkingMemory (Layer 0)
        let memory: Arc<dyn WorkingMemory> =
            Arc::new(SqliteWorkingMemory::new(conn.clone()).await.map_err(|e| {
                AgentError::Internal(format!("Failed to create working memory: {}", e))
            })?);

        // 3. Create SessionStore
        let session_store: Arc<dyn SessionStore> =
            Arc::new(SqliteSessionStore::new(conn.clone()).await.map_err(|e| {
                AgentError::Internal(format!("Failed to create session store: {}", e))
            })?);

        // 4. Create MemoryStore (Layer 2)
        let memory_store: Arc<dyn MemoryStore> = Arc::new(SqliteMemoryStore::new(conn.clone()));

        // 5. Create ToolExecutionRecorder
        let recorder = Arc::new(ToolExecutionRecorder::new(conn));

        // 6. Create Provider
        let api_key = config
            .provider
            .api_key
            .clone()
            .unwrap_or_else(|| std::env::var("ANTHROPIC_API_KEY").unwrap_or_default());
        let provider: Arc<dyn Provider> = Arc::from(create_provider(
            &config.provider.name,
            api_key,
            config.provider.base_url.clone(),
        ));

        // 7. Create ToolRegistry with default + memory + skills
        let mut tools = default_tools();
        register_memory_tools(&mut tools, memory_store.clone(), provider.clone());

        // 8. Create and load SkillRegistry
        let skill_registry = Arc::new(SkillRegistry::new());
        // Load skills from config directories
        if !config.skills_dirs.is_empty() {
            let home_dir = std::env::var("HOME").map(PathBuf::from).ok();
            let project_dir = std::env::current_dir().ok();
            let skill_loader = SkillLoader::new(project_dir.as_deref(), home_dir.as_deref());
            if let Err(e) = skill_registry.load_from(&skill_loader) {
                tracing::warn!("Failed to load skills: {}", e);
            }
            // Register skills as tools
            for skill in skill_registry.invocable_skills() {
                tools.register(SkillTool::new(skill));
            }
            // Start hot-reload watcher
            if let Err(e) = skill_registry.start_watching(skill_loader) {
                tracing::warn!("Failed to start skill watcher: {}", e);
            }
        }

        // 9. Create ProviderChain if configured
        let provider_chain = if let Some(pc_config) = config.provider_chain {
            let chain = Arc::new(ProviderChain::new(pc_config.failover_policy));
            // Note: instances would need to be added separately if needed
            Some(chain)
        } else {
            None
        };

        // 10. EventBus initialization (default enabled)
        let event_bus = if config.enable_event_bus {
            Some(Arc::new(EventBus::new(
                1000,
                1000,
                Arc::new(crate::metrics::MetricsRegistry::new()),
            )))
        } else {
            None
        };

        // 11. McpManager initialization
        let mcp_manager = Arc::new(Mutex::new(McpManager::new()));

        // 12. Working directory
        let working_dir = config
            .working_dir
            .unwrap_or_else(|| PathBuf::from("/tmp/octo-sandbox"));

        // 13. Get default model
        let default_model = config
            .provider
            .model
            .unwrap_or_else(|| "claude-opus-4-5".to_string());

        // 14. Metering initialization (Task 10 - observability)
        let metering = Arc::new(Metering::new());

        Ok(Self {
            primary_handle: Mutex::new(None),
            agent_handles: DashMap::new(),
            catalog,
            provider,
            tools: Arc::new(StdMutex::new(tools)),
            skill_registry: Some(skill_registry),
            memory,
            memory_store,
            session_store,
            default_model,
            event_bus,
            recorder,
            provider_chain,
            mcp_manager,
            working_dir,
            metering,
            tenant_context,
        })
    }

    pub fn with_skill_registry(mut self, skills: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skills);
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = store;
        self
    }

    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = store;
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_recorder(mut self, recorder: Arc<ToolExecutionRecorder>) -> Self {
        self.recorder = recorder;
        self
    }

    pub fn with_provider_chain(mut self, chain: Arc<ProviderChain>) -> Self {
        self.provider_chain = Some(chain);
        self
    }

    // ── Getter 方法（供 server API 层只读访问） ──────────────────────────────

    pub fn catalog(&self) -> &Arc<AgentCatalog> {
        &self.catalog
    }

    pub fn tools(&self) -> &Arc<StdMutex<ToolRegistry>> {
        &self.tools
    }

    pub fn memory(&self) -> &Arc<dyn WorkingMemory> {
        &self.memory
    }

    pub fn memory_store(&self) -> &Arc<dyn MemoryStore> {
        &self.memory_store
    }

    pub fn session_store(&self) -> &Arc<dyn SessionStore> {
        &self.session_store
    }

    pub fn recorder(&self) -> &Arc<ToolExecutionRecorder> {
        &self.recorder
    }

    pub fn provider_chain(&self) -> Option<&Arc<ProviderChain>> {
        self.provider_chain.as_ref()
    }

    pub fn provider(&self) -> &Arc<dyn Provider> {
        &self.provider
    }

    pub fn mcp_manager(&self) -> &Arc<Mutex<crate::mcp::manager::McpManager>> {
        &self.mcp_manager
    }

    /// Get metering snapshot for observability
    pub fn metering(&self) -> crate::metering::MeteringSnapshot {
        self.metering.snapshot()
    }

    /// Get tenant context (if any)
    pub fn tenant_context(&self) -> Option<&TenantContext> {
        self.tenant_context.as_ref()
    }

    /// Verify that the given tenant_id matches the current tenant context.
    /// Returns Ok(()) if access is allowed, or Err(AgentError::PermissionDenied) if not.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to verify access for
    ///
    /// # Returns
    /// * `Ok(())` - If tenant access is allowed (no tenant context set, or matching tenant)
    /// * `Err(AgentError::PermissionDenied)` - If tenant context exists but doesn't match
    pub fn verify_tenant_access(&self, tenant_id: &TenantId) -> Result<(), AgentError> {
        if let Some(ref ctx) = self.tenant_context {
            if &ctx.tenant_id != tenant_id {
                return Err(AgentError::PermissionDenied(format!(
                    "Tenant mismatch: expected {}, got {}",
                    ctx.tenant_id, tenant_id
                )));
            }
        }
        Ok(())
    }

    // ── MCP Server 管理 API ─────────────────────────────────────────────────

    /// 添加 MCP Server → 自动注册 tools
    pub async fn add_mcp_server(
        &self,
        config: crate::mcp::traits::McpServerConfig,
    ) -> Result<Vec<McpToolInfo>, AgentError> {
        let mcp = &self.mcp_manager;

        let tools = {
            let mut guard = mcp.lock().await;
            guard
                .add_server(config.clone())
                .await
                .map_err(|e| AgentError::McpError(e.to_string()))?
        };

        // 注册到 ToolRegistry
        {
            let mcp_guard = mcp.lock().await;
            let mut tools_guard = self.tools.lock().unwrap();
            for tool_info in &tools {
                if let Some(client) = mcp_guard.clients().get(&config.name) {
                    let bridge = crate::mcp::bridge::McpToolBridge::new(
                        client.clone(),
                        config.name.clone(),
                        tool_info.clone(),
                    );
                    tools_guard.register(bridge);
                }
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
            let tools_guard = self.tools.lock().unwrap();
            tools_guard
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let mut new_registry = crate::tools::ToolRegistry::new();
        for (tool_name, tool) in all_tools {
            // 检查工具来源是否为该 MCP server，使用模式匹配
            let should_remove = match tool.source() {
                octo_types::ToolSource::Mcp(server_name) => server_name == name,
                _ => false,
            };
            if should_remove {
                continue; // 跳过要移除的工具
            }
            new_registry.register_arc(tool_name, tool);
        }
        // 替换旧的 registry
        let mut tools_guard = self.tools.lock().unwrap();
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
    pub async fn call_mcp_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let guard = self.mcp_manager.lock().await;
        guard
            .call_tool(server_id, tool_name, arguments)
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

    /// 获取主 AgentExecutorHandle（如果已启动）
    pub async fn primary(&self) -> Option<AgentExecutorHandle> {
        let guard = self.primary_handle.lock().await;
        guard.clone()
    }

    /// 启动主 Runtime 并返回其 Handle。
    /// 由 main.rs 在 server 启动时调用一次。
    /// channels（ws.rs 等）通过持有返回的 Handle 与 Agent 通信，
    /// 无需持有 AgentRuntime 引用（解耦）。
    ///
    /// 如果 primary 已存在，直接返回现有的 handle
    pub async fn start_primary(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
    ) -> AgentExecutorHandle {
        // 先检查是否已有 primary handle
        {
            let guard = self.primary_handle.lock().await;
            if let Some(ref handle) = *guard {
                return handle.clone();
            }
        }

        // 从 manifest 解析运行时配置（不含 tools，使用全局共享引用）
        let (_, system_prompt, model, config) = self.resolve_runtime_config(agent_id);

        let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

        let handle = AgentExecutorHandle {
            tx,
            broadcast_tx: broadcast_tx.clone(),
            session_id: session_id.clone(),
        };

        let runtime = AgentExecutor::new(
            session_id.clone(),
            user_id,
            sandbox_id,
            initial_history,
            rx,
            broadcast_tx,
            self.provider.clone(),
            Arc::clone(&self.tools), // 共享引用，支持 MCP 热插拔
            Arc::new(InMemoryWorkingMemory::new()), // 每 session 独立实例，防止数据污染
            Some(self.memory_store.clone()),
            Some(model),
            Some(self.session_store.clone()),
            system_prompt,
            config,
            self.working_dir.clone(),
            self.event_bus.clone(),
        );

        // Spawn 持久化主循环
        tokio::spawn(async move {
            runtime.run().await;
        });

        if let Some(id) = agent_id {
            let cancel_token = CancellationToken::new();
            self.agent_handles.insert(id.clone(), cancel_token);
            self.catalog.update_state(id, AgentStatus::Running);
        }

        info!(session_id = %session_id.as_str(), "Primary AgentExecutor started");

        // 保存 primary handle
        let mut handle_guard = self.primary_handle.lock().await;
        *handle_guard = Some(handle.clone());

        handle
    }

    /// 停止主 Runtime
    pub async fn stop_primary(&self) {
        let _dropped_handle = {
            let mut guard = self.primary_handle.lock().await;
            guard.take()
        };
        // AgentExecutorHandle is dropped here → tx is dropped → rx.recv() returns None
        // → AgentExecutor while loop naturally exits → tokio task ends
        if _dropped_handle.is_some() {
            info!("Primary AgentExecutor stopped (tx dropped)");
        }
    }

    /// 按 tool_filter 构建 ToolRegistry（含 SkillRegistry 热重载 overlay）
    fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        // 获取 tools 的锁
        let tools_guard = self.tools.lock().unwrap();

        // 快速路径：无动态 skills 且无 filter
        if self.skill_registry.is_none() && tool_filter.is_empty() {
            // 克隆内部的 ToolRegistry 并包装成 Arc
            let mut registry = ToolRegistry::new();
            for (name, tool) in tools_guard.iter() {
                registry.register_arc(name.clone(), tool);
            }
            return Arc::new(registry);
        }

        // 从全局工具快照构建
        let mut registry = ToolRegistry::new();
        for (name, tool) in tools_guard.iter() {
            registry.register_arc(name.clone(), tool);
        }
        drop(tools_guard);

        // 覆盖当前热重载的 skill tools
        if let Some(ref skills) = self.skill_registry {
            for skill in skills.invocable_skills() {
                let name = skill.name.clone();
                registry.register_arc(name, Arc::new(SkillTool::new(skill)));
            }
        }

        // 应用 per-agent tool filter
        if tool_filter.is_empty() {
            return Arc::new(registry);
        }
        let mut filtered = ToolRegistry::new();
        for name in tool_filter {
            if let Some(tool) = registry.get(name) {
                filtered.register_arc(name.clone(), tool);
            }
        }
        Arc::new(filtered)
    }

    /// 从 AgentManifest 构建 system prompt
    fn build_system_prompt(manifest: &AgentManifest) -> Option<String> {
        if let Some(ref prompt) = manifest.system_prompt {
            return Some(prompt.clone());
        }
        if manifest.role.is_some() || manifest.goal.is_some() || manifest.backstory.is_some() {
            let mut parts: Vec<String> = Vec::new();
            if let Some(ref role) = manifest.role {
                parts.push(format!("## Role\n{role}"));
            }
            if let Some(ref goal) = manifest.goal {
                parts.push(format!("## Goal\n{goal}"));
            }
            if let Some(ref backstory) = manifest.backstory {
                parts.push(format!("## Backstory\n{backstory}"));
            }
            return Some(parts.join("\n\n"));
        }
        None // 返回 None 表示使用 AgentLoop 默认（SOUL.md）
    }

    /// 启动 agent：从 catalog 读取 manifest，启动 primary Executor，更新状态机。
    /// session_id：为该 agent 创建或复用的会话标识。
    pub async fn start(
        &self,
        agent_id: &AgentId,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        // 验证 agent 存在
        self.catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;

        // 启动 primary Runtime
        let handle = self
            .start_primary(
                session_id,
                user_id,
                sandbox_id,
                initial_history,
                Some(agent_id),
            )
            .await;

        Ok(handle)
    }

    /// 停止 agent：发送 Cancel，更新 catalog 状态。
    pub async fn stop(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state == AgentStatus::Stopped {
            return Err(AgentError::InvalidTransition {
                from: AgentStatus::Stopped,
                action: "stop",
            });
        }
        if let Some((_, token)) = self.agent_handles.remove(agent_id) {
            token.cancel();
        }
        let handle = {
            let mut guard = self.primary_handle.lock().await;
            guard.take()
        };
        if let Some(ref h) = handle {
            if let Err(e) = h.send(AgentMessage::Cancel).await {
                tracing::warn!("cancel send failed on stop: {e}");
            }
        }
        self.catalog.update_state(agent_id, AgentStatus::Stopped);
        Ok(())
    }

    /// 暂停 agent：发送 Cancel（中断当前 round），更新 catalog 状态。
    pub async fn pause(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state != AgentStatus::Running {
            return Err(AgentError::InvalidTransition {
                from: entry.state.clone(),
                action: "pause",
            });
        }
        if let Some((_, token)) = self.agent_handles.remove(agent_id) {
            token.cancel();
        }
        let handle = {
            let guard = self.primary_handle.lock().await;
            guard.clone()
        };
        if let Some(ref h) = handle {
            if let Err(e) = h.send(AgentMessage::Cancel).await {
                tracing::warn!("cancel send failed on pause: {e}");
            }
        }
        self.catalog.update_state(agent_id, AgentStatus::Paused);
        Ok(())
    }

    /// 恢复 agent：更新 catalog 状态（Runtime 仍在运行，cancel_flag 已重置）。
    pub fn resume(&self, agent_id: &AgentId) -> Result<(), AgentError> {
        let entry = self
            .catalog
            .get(agent_id)
            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
        if entry.state != AgentStatus::Paused {
            return Err(AgentError::InvalidTransition {
                from: entry.state.clone(),
                action: "resume",
            });
        }
        let cancel_token = CancellationToken::new();
        self.agent_handles.insert(agent_id.clone(), cancel_token);
        self.catalog.update_state(agent_id, AgentStatus::Running);
        Ok(())
    }

    /// 按 agent_id 解析运行时配置（从 catalog 读取 manifest）
    fn resolve_runtime_config(
        &self,
        agent_id: Option<&AgentId>,
    ) -> (Arc<ToolRegistry>, Option<String>, String, AgentConfig) {
        if let Some(id) = agent_id {
            if let Some(entry) = self.catalog.get(id) {
                let manifest = &entry.manifest;
                let tools = self.build_tool_registry(&manifest.tool_filter);
                let system_prompt = Self::build_system_prompt(manifest);
                let model = manifest
                    .model
                    .clone()
                    .unwrap_or_else(|| self.default_model.clone());
                let config = manifest.config.clone();
                return (tools, system_prompt, model, config);
            }
        }
        // 无 agent_id 或 agent 不存在：使用全局默认
        {
            let tools_guard = self.tools.lock().unwrap();
            let mut registry = ToolRegistry::new();
            for (name, tool) in tools_guard.iter() {
                registry.register_arc(name.clone(), tool);
            }
            (
                Arc::new(registry),
                None,
                self.default_model.clone(),
                AgentConfig::default(),
            )
        }
    }

    /// Execute a scheduled task: create session, run agent, return result.
    /// Reuses provider/tools/memory from this AgentRuntime.
    pub async fn execute_scheduled_task(&self, task: &ScheduledTask) -> Result<String, AgentError> {
        let config = &task.agent_config;

        // Create session for the task using the session store
        let user_id = task
            .user_id
            .as_ref()
            .map(|u| UserId::from_string(u.clone()))
            .unwrap_or_else(|| UserId::from_string("scheduler".to_string()));

        let session = self.session_store.create_session_with_user(&user_id).await;
        let session_id = session.session_id.clone();
        let sandbox_id = session.sandbox_id.clone();

        // Prepare initial message with the task input
        let user_message = ChatMessage::user(config.input.clone());
        let mut messages = vec![user_message];

        // Create tool context
        let tool_ctx = ToolContext {
            sandbox_id: sandbox_id.clone(),
            working_dir: PathBuf::from("/tmp"),
        };

        // Create event channel (discard events)
        let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(100);

        // Create and configure agent loop using this runtime's dependencies
        let tools_guard = self.tools.lock().unwrap();
        let mut registry = ToolRegistry::new();
        for (name, tool) in tools_guard.iter() {
            registry.register_arc(name.clone(), tool);
        }
        let tools = Arc::new(registry);
        drop(tools_guard);

        let mut agent_loop = AgentLoop::new(self.provider.clone(), tools, self.memory.clone())
            .with_model(config.model.clone());

        // Run agent with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout_secs),
            agent_loop.run(
                &session_id,
                &user_id,
                &sandbox_id,
                &mut messages,
                tx,
                tool_ctx,
                None,
            ),
        )
        .await;

        match result {
            Ok(Ok(_)) => {
                // Extract response from last assistant message
                let response = messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .and_then(|m| {
                        m.content.iter().find_map(|c| {
                            if let ContentBlock::Text { text } = c {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_else(|| "Task completed".to_string());

                tracing::info!(
                    task_id = %task.id,
                    session_id = %session_id,
                    "Scheduled task completed successfully"
                );

                Ok(response)
            }
            Ok(Err(e)) => {
                tracing::error!(task_id = %task.id, error = %e, "Agent execution error");
                Err(AgentError::ScheduledTask(e.to_string()))
            }
            Err(_) => {
                tracing::error!(task_id = %task.id, "Agent execution timed out");
                Err(AgentError::ScheduledTask(format!(
                    "Timeout after {} seconds",
                    config.timeout_secs
                )))
            }
        }
    }
}
