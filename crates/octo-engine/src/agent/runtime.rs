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
use crate::event::{EventStore, TelemetryBus};
use crate::hooks::HookRegistry;
use crate::mcp::manager::McpManager;
use crate::memory::store_traits::MemoryStore;
use crate::memory::{InMemoryWorkingMemory, SqliteMemoryStore, SqliteWorkingMemory, WorkingMemory};
use crate::metering::Metering;
use crate::providers::ProviderConfig;
use crate::providers::{
    create_provider, defaults::resolve_api_key_env, Provider, ProviderChain, ProviderChainConfig,
};
use crate::sandbox::{DockerAdapter, OctoRunMode, SandboxProfile, SessionSandboxConfig, SessionSandboxManager};
use crate::security::SecurityPolicy;
use crate::session::{SessionStore, SqliteSessionStore};
use crate::skills::{
    register_skills_as_tools, sync_builtin_skills, ExecuteSkillTool, SkillLoader, SkillRegistry,
    SkillTool,
};
use crate::tools::recorder::ToolExecutionRecorder;
use crate::tools::{
    default_tools, register_kg_tools, register_memory_tools, register_working_memory_tools, register_scheduler_tools, ToolRegistry,
};

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
    /// Optional OctoRoot for unified path management
    pub octo_root: Option<crate::root::OctoRoot>,
    /// Sandbox profile override (development/staging/production)
    pub sandbox_profile: Option<String>,
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
            octo_root: None,
            sandbox_profile: None,
        }
    }

    /// Set the OctoRoot for unified path management.
    pub fn with_octo_root(mut self, root: crate::root::OctoRoot) -> Self {
        self.octo_root = Some(root);
        self
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
    pub(crate) event_bus: Option<Arc<TelemetryBus>>,
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
    // Persistent event store (wired into TelemetryBus)
    pub(crate) event_store: Option<Arc<EventStore>>,
    // Agent router for task-to-agent matching
    router: tokio::sync::RwLock<crate::agent::router::AgentRouter>,
    // Default SafetyPipeline with CanaryGuardLayer (T1)
    pub(crate) safety_pipeline: Option<Arc<crate::security::SafetyPipeline>>,
    // Canary token for system prompt injection (T1)
    pub(crate) canary_token: Option<String>,
    // Shared approval gate for pending human approval requests (T7)
    pub(crate) approval_gate: Option<crate::tools::approval::ApprovalGate>,
    // Optional collaboration manager for multi-agent sessions (T9)
    pub(crate) collaboration_manager:
        Option<Arc<Mutex<crate::agent::collaboration::manager::CollaborationManager>>>,
    // Knowledge graph for entity-relation storage (Wave 10 C1)
    pub(crate) knowledge_graph: Arc<tokio::sync::RwLock<crate::memory::KnowledgeGraph>>,
    // Credential resolver for secure secret resolution (Vault > .env > env vars)
    pub(crate) credential_resolver: Arc<crate::secret::CredentialResolver>,
    // Session-scoped sandbox container manager (Phase AF)
    pub(crate) session_sandbox: Option<Arc<SessionSandboxManager>>,
    // Session summary store for episodic memory (Phase AG)
    pub(crate) session_summary_store: Option<Arc<crate::memory::SessionSummaryStore>>,
}

impl AgentRuntime {
    /// Create a new AgentRuntime with all components internalized.
    ///
    /// # Arguments
    /// * `catalog` - Agent catalog (created externally with store)
    /// * `config` - Runtime configuration containing db_path, provider, skills, etc.
    /// * `tenant_context` - Optional tenant context for multi-tenant isolation.
    ///   Pass `None` for single-user mode (octo-workbench).
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

        // 4b. Create SessionSummaryStore (Phase AG — episodic memory)
        let session_summary_store = Some(Arc::new(
            crate::memory::SessionSummaryStore::new(conn.clone()),
        ));

        // 5. Create ToolExecutionRecorder
        let recorder = Arc::new(ToolExecutionRecorder::new(conn.clone()));

        // 5b. CredentialResolver (Vault > env > credentials.yaml > .env)
        let credential_resolver = {
            let mut resolver = crate::secret::CredentialResolver::new();
            if let Ok(password) = std::env::var("OCTO_VAULT_PASSWORD") {
                match crate::secret::CredentialVault::new(password) {
                    Ok(vault) => {
                        tracing::info!("CredentialVault initialized (AES-GCM encrypted)");
                        resolver = resolver.with_vault(vault);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to init CredentialVault, using env-only resolver");
                    }
                }
            }
            // Wire credentials.yaml from OctoRoot (populated by `octo auth login`)
            if let Some(ref root) = config.octo_root {
                let creds = root.credentials_path();
                if creds.exists() {
                    tracing::debug!(path = %creds.display(), "Loading credentials.yaml");
                    resolver = resolver.with_credentials(creds);
                }
            }
            Arc::new(resolver)
        };

        // 6. Create Provider (resolve API key via CredentialResolver priority chain)
        let api_key = config
            .provider
            .api_key
            .clone()
            .unwrap_or_else(|| {
                let env_key = resolve_api_key_env(&config.provider.name)
                    .unwrap_or("ANTHROPIC_API_KEY");
                credential_resolver
                    .resolve(env_key)
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            });
        tracing::info!(
            provider = %config.provider.name,
            base_url = ?config.provider.base_url,
            model = ?config.provider.model,
            api_key_len = api_key.len(),
            "Creating LLM provider"
        );
        let provider: Arc<dyn Provider> = Arc::from(create_provider(
            &config.provider.name,
            api_key,
            config.provider.base_url.clone(),
        ));

        // 7. Create ToolRegistry with default + memory + knowledge graph + skills
        let mut tools = default_tools();
        register_memory_tools(&mut tools, memory_store.clone(), provider.clone());
        register_working_memory_tools(&mut tools, memory.clone());

        // 7b. Create KnowledgeGraph and register KG tools
        let knowledge_graph = Arc::new(tokio::sync::RwLock::new(
            crate::memory::KnowledgeGraph::new(),
        ));
        register_kg_tools(&mut tools, knowledge_graph.clone());

        // 7c. Create SchedulerStorage and register scheduler tools
        let scheduler_storage: Arc<dyn crate::scheduler::SchedulerStorage> = Arc::new(
            crate::scheduler::SqliteSchedulerStorage::new(conn),
        );
        register_scheduler_tools(&mut tools, scheduler_storage);

        // 8. Create and load SkillRegistry
        let skill_registry = Arc::new(SkillRegistry::new());
        // Determine skills loading paths from OctoRoot (if available) or legacy config
        let should_load_skills = config.octo_root.is_some() || !config.skills_dirs.is_empty();
        if should_load_skills {
            // Resolve project_dir and home_dir from OctoRoot or fallback
            let (project_dir, home_dir) = if let Some(ref root) = config.octo_root {
                (
                    Some(root.working_dir().to_path_buf()),
                    Some(
                        root.global_root()
                            .parent()
                            .unwrap_or(root.global_root())
                            .to_path_buf(),
                    ),
                )
            } else {
                (std::env::current_dir().ok(), dirs::home_dir())
            };

            // Sync builtin skills to global ~/.octo/skills/ (never overwrites existing)
            let global_skills_dir = if let Some(ref root) = config.octo_root {
                root.global_skills_dir()
            } else {
                home_dir
                    .as_ref()
                    .map(|h| h.join(".octo").join("skills"))
                    .unwrap_or_else(|| PathBuf::from(".octo/skills"))
            };
            if let Err(e) = std::fs::create_dir_all(&global_skills_dir) {
                tracing::warn!("Failed to create global skills dir: {}", e);
            } else if let Err(e) = sync_builtin_skills(&global_skills_dir) {
                tracing::warn!("Failed to sync builtin skills: {}", e);
            }

            let skill_loader = SkillLoader::new(project_dir.as_deref(), home_dir.as_deref());
            if let Err(e) = skill_registry.load_from(&skill_loader) {
                tracing::warn!("Failed to load skills: {}", e);
            }
            // Register user-invocable skills as tools via bridge
            register_skills_as_tools(&skill_loader, &mut tools);
            // Register execute_skill tool
            tools.register(ExecuteSkillTool::new(skill_registry.clone()));
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

        // 10. TelemetryBus + EventStore initialization
        let db2 = Database::open(&config.db_path)
            .await
            .map_err(|e| AgentError::Internal(format!("Failed to open event DB: {}", e)))?;
        let event_store = Arc::new(
            EventStore::new(db2.conn().clone())
                .await
                .map_err(|e| AgentError::Internal(format!("Failed to create EventStore: {}", e)))?,
        );

        let event_bus = if config.enable_event_bus {
            Some(Arc::new(
                TelemetryBus::new(
                    1000,
                    1000,
                    Arc::new(crate::metrics::MetricsRegistry::new()),
                )
                .with_event_store(event_store.clone()),
            ))
        } else {
            None
        };

        // 11. McpManager initialization — auto-load from mcp.json if available
        let mcp_manager = {
            let mut mgr = McpManager::new();

            // Collect config paths to load (highest priority first):
            // 1. $PROJECT/.octo/mcp.json    (octo-native, project-level)
            // 2. $PROJECT/.mcp.json         (CC-compatible, project-level)
            // 3. ~/.octo/mcp/mcp.json       (octo-native, global)
            let mut config_paths = Vec::new();
            if let Some(ref root) = config.octo_root {
                let project_mcp = root.project_root().join("mcp.json");
                let cc_compat_mcp = root.working_dir().join(".mcp.json");
                let global_mcp = root.global_mcp_dir().join("mcp.json");
                if project_mcp.exists() {
                    config_paths.push(project_mcp);
                }
                if cc_compat_mcp.exists() {
                    config_paths.push(cc_compat_mcp);
                }
                if global_mcp.exists() {
                    config_paths.push(global_mcp);
                }
            }

            let mut loaded_names = std::collections::HashSet::new();
            for path in &config_paths {
                match McpManager::load_config(path) {
                    Ok(configs) => {
                        for server_config in configs {
                            if loaded_names.contains(&server_config.name) {
                                tracing::debug!(
                                    server = %server_config.name,
                                    path = %path.display(),
                                    "Skipping duplicate MCP server (already loaded from higher-priority config)"
                                );
                                continue;
                            }
                            let name = server_config.name.clone();
                            match mgr.add_server(server_config).await {
                                Ok(tools) => {
                                    tracing::info!(
                                        server = %name,
                                        tools = tools.len(),
                                        config = %path.display(),
                                        "Auto-loaded MCP server from config"
                                    );
                                    loaded_names.insert(name);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        server = %name,
                                        config = %path.display(),
                                        error = %e,
                                        "Failed to auto-load MCP server (will continue without it)"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to parse MCP config file"
                        );
                    }
                }
            }

            if !loaded_names.is_empty() {
                tracing::info!(
                    count = loaded_names.len(),
                    servers = ?loaded_names,
                    "MCP servers auto-loaded from config"
                );
            }

            Arc::new(Mutex::new(mgr))
        };

        // 12. Working directory
        let working_dir = config.working_dir.unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp/octo-sandbox"))
        });

        // 13. Get default model
        let default_model = config
            .provider
            .model
            .unwrap_or_else(|| "claude-opus-4-5".to_string());

        // 14. Metering initialization (Task 10 - observability)
        let metering = Arc::new(Metering::new());

        // 15. SecurityPolicy initialization (path validation for ToolContext)
        let security_policy = Arc::new(SecurityPolicy::new().with_working_dir(working_dir.clone()));

        // 16. SafetyPipeline with CanaryGuardLayer (T1 — canary token injection)
        let canary_guard = crate::security::CanaryGuardLayer::with_default_canary();
        let canary_token = canary_guard.canary().to_string();
        let safety_pipeline = Arc::new(
            crate::security::SafetyPipeline::new().add_layer(Box::new(canary_guard)),
        );

        // 17. Shared ApprovalGate for interactive tool approval (T7)
        let approval_gate = crate::tools::approval::ApprovalGate::new();

        // 18. (CredentialResolver moved to step 5b — before provider creation)

        // 19. SessionSandboxManager (SSM) — conditional on run mode + profile
        let session_sandbox: Option<Arc<SessionSandboxManager>> = {
            let run_mode = OctoRunMode::detect();
            let profile = SandboxProfile::resolve(
                false,
                config.sandbox_profile.as_deref(),
                None,
            );
            if run_mode == OctoRunMode::Host && profile != SandboxProfile::Development {
                // Attempt to create DockerAdapter for container-backed sandbox
                let docker = DockerAdapter::new(crate::sandbox::DEFAULT_SANDBOX_IMAGE);
                let mut ssm_config = SessionSandboxConfig::default();
                // Pass host working directory for bind mount into container
                ssm_config.host_working_dir = Some(working_dir.clone());
                let ssm = SessionSandboxManager::new(Arc::new(docker), ssm_config);
                tracing::info!(
                    run_mode = %run_mode,
                    profile = %profile,
                    "SessionSandboxManager initialized (Docker backend)"
                );
                Some(Arc::new(ssm))
            } else {
                tracing::debug!(
                    run_mode = %run_mode,
                    profile = %profile,
                    "SessionSandboxManager skipped (development mode or already sandboxed)"
                );
                None
            }
        };

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
            working_dir: working_dir.clone(),
            metering,
            security_policy: security_policy.clone(),
            event_store: Some(event_store),
            hook_registry: {
                let registry = Arc::new(HookRegistry::new());
                // Register builtin + declarative + policy handlers (Phase AH)
                {
                    let r = registry.clone();
                    let sp = security_policy;
                    let wd = working_dir.clone();
                    tokio::spawn(async move {
                        use crate::hooks::HookPoint;

                        // Layer 1: Builtin handlers
                        r.register(
                            HookPoint::PreToolUse,
                            Arc::new(crate::hooks::builtin::SecurityPolicyHandler::new(sp)),
                        ).await;
                        r.register(
                            HookPoint::PostToolUse,
                            Arc::new(crate::hooks::builtin::AuditLogHandler),
                        ).await;
                        tracing::debug!("Layer 1: builtin handlers registered (security-policy, audit-log)");

                        // Layer 2: Policy engine (policies.yaml)
                        if let Some(policies_path) = resolve_policies_path(Some(&wd)) {
                            match crate::hooks::policy::config::load_policies_config(&policies_path) {
                                Ok(policy_config) => {
                                    let pc = Arc::new(policy_config);
                                    // Register PolicyEngineBridge for each unique hook point
                                    let hook_points = collect_policy_hook_points(&pc);
                                    for hp in hook_points {
                                        r.register(
                                            hp,
                                            Arc::new(crate::hooks::policy::PolicyEngineBridge::new(pc.clone(), hp)),
                                        ).await;
                                    }
                                    tracing::info!(path = %policies_path.display(), "Layer 2: policy engine loaded");
                                }
                                Err(e) => {
                                    tracing::warn!(path = %policies_path.display(), error = %e, "Failed to load policies.yaml");
                                }
                            }
                        }

                        // Layer 3: Declarative hooks (hooks.yaml)
                        if let Some(hooks_config) = crate::hooks::declarative::loader::load_hooks_config_auto(Some(&wd)) {
                            let hc = Arc::new(hooks_config);
                            // Register DeclarativeHookBridge for each configured hook point
                            let hook_points = collect_declarative_hook_points(&hc);
                            for hp in hook_points {
                                r.register(
                                    hp,
                                    Arc::new(crate::hooks::declarative::DeclarativeHookBridge::new(hc.clone(), hp)),
                                ).await;
                            }
                            tracing::info!("Layer 3: declarative hooks loaded");
                        }
                    });
                }
                registry
            },
            tenant_context,
            router: tokio::sync::RwLock::new(crate::agent::router::AgentRouter::new()),
            safety_pipeline: Some(safety_pipeline),
            canary_token: Some(canary_token),
            approval_gate: Some(approval_gate),
            collaboration_manager: None,
            knowledge_graph,
            credential_resolver,
            session_sandbox,
            session_summary_store,
        };

        // 17. Load declarative YAML agent definitions (if configured)
        if let Some(ref dir) = config.agents_dir {
            let loader = crate::agent::AgentManifestLoader::new(dir);
            match loader.load_all(&runtime.catalog) {
                Ok(n) => tracing::info!(count = n, "Loaded YAML agent manifests"),
                Err(e) => tracing::warn!(error = %e, "Failed to load agent YAML manifests"),
            }
        }

        // 18. Register MCP management tools (mcp_install, mcp_remove, mcp_list)
        {
            let mcp_config_path = config
                .octo_root
                .as_ref()
                .map(|r| r.project_root().join("mcp.json"))
                .unwrap_or_else(|| PathBuf::from(".octo/mcp.json"));
            let handle = crate::tools::mcp_manage::McpManageHandle {
                mcp_manager: runtime.mcp_manager.clone(),
                tools: runtime.tools.clone(),
                config_path: mcp_config_path,
            };
            let mut tools_guard = runtime.tools.lock().unwrap_or_else(|e| e.into_inner());
            tools_guard.register(crate::tools::mcp_manage::McpInstallTool::new(handle.clone()));
            tools_guard.register(crate::tools::mcp_manage::McpRemoveTool::new(handle.clone()));
            tools_guard.register(crate::tools::mcp_manage::McpListTool::new(handle));
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

    pub fn with_event_bus(mut self, bus: Arc<TelemetryBus>) -> Self {
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

    /// Get event store (if any)
    pub fn event_store(&self) -> Option<&Arc<EventStore>> {
        self.event_store.as_ref()
    }

    /// Get canary token (if any)
    pub fn canary_token(&self) -> Option<&str> {
        self.canary_token.as_deref()
    }

    /// Get safety pipeline (if any)
    pub fn safety_pipeline(&self) -> Option<&Arc<crate::security::SafetyPipeline>> {
        self.safety_pipeline.as_ref()
    }

    /// Get credential resolver for secure secret resolution
    pub fn credential_resolver(&self) -> &Arc<crate::secret::CredentialResolver> {
        &self.credential_resolver
    }

    /// Get shared approval gate (if any) — T7
    pub fn approval_gate(&self) -> Option<&crate::tools::approval::ApprovalGate> {
        self.approval_gate.as_ref()
    }

    /// Get collaboration context (if a collaboration session is active) — T9.
    pub fn collaboration_context(
        &self,
    ) -> Option<Arc<crate::agent::collaboration::context::CollaborationContext>> {
        // We try_lock to avoid blocking; if it's locked, collaboration is in use.
        let mgr_arc = self.collaboration_manager.as_ref()?;
        let guard = mgr_arc.try_lock().ok()?;
        Some(Arc::clone(guard.context()))
    }

    /// Get snapshot of collaboration agents (empty vec if no active collaboration) — T9.
    pub fn collaboration_agents(
        &self,
    ) -> Vec<crate::agent::collaboration::manager::CollaborationAgent> {
        let mgr_arc = match &self.collaboration_manager {
            Some(m) => m,
            None => return vec![],
        };
        match mgr_arc.try_lock() {
            Ok(guard) => guard.agents(),
            Err(_) => vec![],
        }
    }

    /// Set the collaboration manager for this runtime — T9.
    pub fn set_collaboration_manager(
        &mut self,
        mgr: Arc<Mutex<crate::agent::collaboration::manager::CollaborationManager>>,
    ) {
        self.collaboration_manager = Some(mgr);
    }

    /// Delete expired memory entries (convenience wrapper).
    pub async fn cleanup_expired_memories(&self) -> anyhow::Result<usize> {
        self.memory_store.delete_expired().await
    }

    /// Get knowledge graph
    pub fn knowledge_graph(&self) -> &Arc<tokio::sync::RwLock<crate::memory::KnowledgeGraph>> {
        &self.knowledge_graph
    }

    /// Get skill registry (if any)
    pub fn skill_registry(&self) -> Option<&Arc<SkillRegistry>> {
        self.skill_registry.as_ref()
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
            self.safety_pipeline.clone(),
            self.canary_token.clone(),
            self.approval_gate.clone(),
            self.skill_registry.clone(),
            Some(self.recorder.clone()),
            self.session_sandbox.clone(), // AF-T1: SSM wired from AgentRuntime
            self.session_summary_store.clone(), // Phase AG: SessionSummaryStore
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

// ---------------------------------------------------------------------------
// Phase AH: Helper functions for hook system initialization
// ---------------------------------------------------------------------------

/// Resolve policies.yaml path using the same layered strategy as hooks.yaml.
fn resolve_policies_path(project_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    // 1. Environment variable override
    if let Ok(env_path) = std::env::var("OCTO_POLICIES_FILE") {
        let p = std::path::PathBuf::from(&env_path);
        if p.exists() {
            return Some(p);
        }
    }
    // 2. Project-level config
    if let Some(project) = project_dir {
        let project_policies = project.join(".octo").join("policies.yaml");
        if project_policies.exists() {
            return Some(project_policies);
        }
    }
    // 3. Global config
    if let Some(home) = dirs::home_dir() {
        let global_policies = home.join(".octo").join("policies.yaml");
        if global_policies.exists() {
            return Some(global_policies);
        }
    }
    None
}

/// Collect unique HookPoints referenced by policies in a PolicyConfig.
fn collect_policy_hook_points(
    config: &crate::hooks::policy::PolicyConfig,
) -> Vec<crate::hooks::HookPoint> {
    let mut points = std::collections::HashSet::new();
    for policy in &config.policies {
        if !policy.enabled {
            continue;
        }
        for hook_name in &policy.hooks {
            if let Some(hp) = hook_point_from_name(hook_name) {
                points.insert(hp);
            }
        }
    }
    points.into_iter().collect()
}

/// Collect unique HookPoints referenced by entries in a HooksConfig.
fn collect_declarative_hook_points(
    config: &crate::hooks::declarative::HooksConfig,
) -> Vec<crate::hooks::HookPoint> {
    let mut points = Vec::new();
    for key in config.hooks.keys() {
        if let Some(hp) = hook_point_from_name(key) {
            points.push(hp);
        }
    }
    points
}

/// Convert a hook point name string to a HookPoint enum variant.
fn hook_point_from_name(name: &str) -> Option<crate::hooks::HookPoint> {
    use crate::hooks::HookPoint;
    match name {
        "PreToolUse" => Some(HookPoint::PreToolUse),
        "PostToolUse" => Some(HookPoint::PostToolUse),
        "PreTask" => Some(HookPoint::PreTask),
        "PostTask" => Some(HookPoint::PostTask),
        "SessionStart" => Some(HookPoint::SessionStart),
        "SessionEnd" => Some(HookPoint::SessionEnd),
        "ContextDegraded" => Some(HookPoint::ContextDegraded),
        "LoopTurnStart" => Some(HookPoint::LoopTurnStart),
        "LoopTurnEnd" => Some(HookPoint::LoopTurnEnd),
        "AgentRoute" => Some(HookPoint::AgentRoute),
        "SkillsActivated" => Some(HookPoint::SkillsActivated),
        "SkillDeactivated" => Some(HookPoint::SkillDeactivated),
        "SkillScriptStarted" => Some(HookPoint::SkillScriptStarted),
        "ToolConstraintViolated" => Some(HookPoint::ToolConstraintViolated),
        _ => {
            tracing::warn!(name, "Unknown hook point name in config, skipping");
            None
        }
    }
}
