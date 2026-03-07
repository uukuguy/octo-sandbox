use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::info;

use octo_types::{ChatMessage, SandboxId, SessionId, TenantId, UserId};

use crate::agent::{
    AgentCatalog, AgentConfig, AgentError, AgentEvent, AgentExecutor, AgentExecutorHandle, AgentId,
    AgentManifest, AgentMessage, AgentStatus, CancellationToken, TenantContext,
};
use crate::db::Database;
use crate::event::EventBus;
use crate::hooks::HookRegistry;
use crate::mcp::manager::McpManager;
use crate::memory::store_traits::MemoryStore;
use crate::memory::{InMemoryWorkingMemory, SqliteMemoryStore, SqliteWorkingMemory, WorkingMemory};
use crate::metering::Metering;
use crate::providers::ProviderConfig;
use crate::providers::{create_provider, Provider, ProviderChain, ProviderChainConfig};
use crate::security::SecurityPolicy;
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
    /// Optional directory to scan for declarative YAML agent definitions
    pub agents_dir: Option<std::path::PathBuf>,
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
            agents_dir: None,
        }
    }
}

/// Session → AgentExecutorHandle 的注册表，同时持有所有共享运行时依赖
pub struct AgentRuntime {
    /// 单一主 executor（单用户场景）- 使用 Mutex 实现内部可变性
    pub(crate) primary_handle: Mutex<Option<AgentExecutorHandle>>,
    /// AgentId → CancellationToken，用于 stop/pause 时取消正在运行的 AgentExecutor
    pub(crate) agent_handles: DashMap<AgentId, CancellationToken>,
    // 定义层
    pub(crate) catalog: Arc<AgentCatalog>,
    // 共享依赖（构造时注入一次）
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) tools: Arc<StdMutex<ToolRegistry>>,
    pub(crate) skill_registry: Option<Arc<SkillRegistry>>,
    pub(crate) memory: Arc<dyn WorkingMemory>,
    pub(crate) memory_store: Arc<dyn MemoryStore>,
    pub(crate) session_store: Arc<dyn SessionStore>,
    pub(crate) default_model: String,
    // Observability: event bus already forwarded to AgentExecutor at line 482
    pub(crate) event_bus: Option<Arc<EventBus>>,
    pub(crate) recorder: Arc<ToolExecutionRecorder>,
    pub(crate) provider_chain: Option<Arc<ProviderChain>>,
    // Runtime fields (Task 2)
    pub(crate) mcp_manager: Arc<Mutex<crate::mcp::manager::McpManager>>,
    pub(crate) working_dir: PathBuf,
    // Observability: metering for token usage tracking
    pub(crate) metering: Arc<Metering>,
    // Security policy for path validation (injected into ToolContext)
    pub(crate) security_policy: Arc<SecurityPolicy>,
    // Hook system
    pub(crate) hook_registry: Arc<HookRegistry>,
    // Tenant isolation (Task 3)
    pub(crate) tenant_context: Option<TenantContext>,
    // Agent router for task-to-agent matching
    router: tokio::sync::RwLock<crate::agent::router::AgentRouter>,
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

        // 15. SecurityPolicy initialization (path validation for ToolContext)
        let security_policy = Arc::new(
            SecurityPolicy::new().with_workspace(working_dir.clone()),
        );

        let runtime = Self {
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
            security_policy,
            hook_registry: Arc::new(HookRegistry::new()),
            tenant_context,
            router: tokio::sync::RwLock::new(crate::agent::router::AgentRouter::new()),
        };

        // 16. Load declarative YAML agent definitions (if configured)
        if let Some(ref dir) = config.agents_dir {
            let loader = crate::agent::AgentManifestLoader::new(dir);
            match loader.load_all(&runtime.catalog) {
                Ok(n) => tracing::info!(count = n, "Loaded YAML agent manifests"),
                Err(e) => tracing::warn!(error = %e, "Failed to load agent YAML manifests"),
            }
        }

        Ok(runtime)
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

    /// Get security policy
    pub fn security_policy(&self) -> &Arc<SecurityPolicy> {
        &self.security_policy
    }

    /// Get hook registry
    pub fn hook_registry(&self) -> &Arc<HookRegistry> {
        &self.hook_registry
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
        // Hold the lock for the entire operation to prevent TOCTOU races.
        // Two concurrent callers cannot both pass the "already started" check
        // and each create a separate executor.
        let mut handle_guard = self.primary_handle.lock().await;

        // Return existing handle if already started
        if let Some(ref handle) = *handle_guard {
            return handle.clone();
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
            Some(self.security_policy.clone() as Arc<dyn octo_types::PathValidator>),
            Some(self.hook_registry.clone()),
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

        // Store handle and return — all within the same lock scope
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

    /// Register an agent's capabilities with the router.
    pub async fn router_register(&self, profile: crate::agent::router::AgentProfile) {
        self.router.write().await.register(profile);
    }

    /// Remove a registered agent from the router by agent_id.
    pub async fn router_unregister(&self, agent_id: &str) {
        self.router.write().await.unregister(agent_id);
    }

    /// Route a task description to the best matching agent.
    /// Returns `None` if no agents are registered.
    pub async fn route_task(&self, task: &str) -> Option<crate::agent::router::RouteResult> {
        self.router.read().await.route(task)
    }

    /// Register an agent manifest's capabilities with the router using its profile.
    pub async fn router_register_manifest(
        &self,
        agent_id: impl Into<String>,
        manifest: &crate::agent::AgentManifest,
    ) {
        let profile = manifest.to_agent_profile(agent_id);
        self.router.write().await.register(profile);
    }

    /// 按 tool_filter 构建 ToolRegistry（含 SkillRegistry 热重载 overlay）
    fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        // 获取 tools 的锁
        let tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());

        // 快速路径：无动态 skills 且无 filter
        if self.skill_registry.is_none() && tool_filter.is_empty() {
            return Arc::new(tools_guard.snapshot());
        }

        // 从全局工具快照构建
        let mut registry = tools_guard.snapshot();
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
            let tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            (
                Arc::new(tools_guard.snapshot()),
                None,
                self.default_model.clone(),
                AgentConfig::default(),
            )
        }
    }

}
