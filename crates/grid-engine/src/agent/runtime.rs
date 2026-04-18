use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::info;

use grid_types::{ChatMessage, SandboxId, SessionId, TenantId, UserId};

use crate::agent::{
    AgentCatalog, AgentConfig, AgentError, AgentEvent, AgentExecutor, AgentExecutorHandle, AgentId,
    AgentManifest, AgentMessage, AgentStatus, CancellationToken, CancellationTokenTree,
    SessionInterruptRegistry, TenantContext,
};
use crate::agent::task_tracker::TaskTracker;
use crate::agent::team::TeamManager;
use crate::db::Database;
use crate::event::{EventStore, TelemetryBus};
use crate::hooks::HookRegistry;
use crate::mcp::manager::McpManager;
use crate::mcp::stdio::StdioMcpClient;
use crate::mcp::traits::McpClient as _;
use crate::memory::store_traits::MemoryStore;
use crate::memory::{InMemoryWorkingMemory, SqliteMemoryStore, SqliteWorkingMemory, WorkingMemory};
use crate::metering::Metering;
use crate::providers::ProviderConfig;
use crate::providers::{
    create_provider, defaults::resolve_api_key_env, Provider, ProviderChain, ProviderChainConfig,
};
use crate::sandbox::{DockerAdapter, GridRunMode, SandboxProfile, SessionSandboxConfig, SessionSandboxManager};
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
// BROADCAST_CAPACITY must be large enough to absorb a full skill workflow's
// events (thinking + text_delta + tool_start + tool_result + final Done)
// without the slow gRPC consumer falling behind. A 180s workflow can emit
// 500+ events; 256 was too small — the consumer lagged, oldest events
// (including `AgentEvent::Done`) were dropped, and `map_events_to_chunks`
// never saw a "done" chunk to terminate the gRPC stream, leaving the CLI
// hanging on `session send`. See also the Lag-fallback in
// `GridHarness::map_events_to_chunks` for defense-in-depth.
const BROADCAST_CAPACITY: usize = 4096;

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
    /// Optional GridRoot for unified path management
    pub grid_root: Option<crate::root::GridRoot>,
    /// Sandbox profile override (development/staging/production)
    pub sandbox_profile: Option<String>,
    /// Maximum concurrent sessions (Phase AJ-T5, default: 64)
    pub max_concurrent_sessions: Option<usize>,
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
            grid_root: None,
            sandbox_profile: None,
            max_concurrent_sessions: None,
        }
    }

    /// Set the GridRoot for unified path management.
    pub fn with_grid_root(mut self, root: crate::root::GridRoot) -> Self {
        self.grid_root = Some(root);
        self
    }
}

/// Default maximum concurrent sessions
const DEFAULT_MAX_CONCURRENT_SESSIONS: usize = 64;

/// 多会话注册表中的会话条目（Phase AJ-T5）
pub struct SessionEntry {
    /// AgentExecutor handle for sending messages and subscribing to events
    pub handle: AgentExecutorHandle,
    /// User who owns this session
    pub user_id: UserId,
    /// When this session was created
    pub created_at: Instant,
    /// Session-level tool registry (isolated from other sessions)
    pub tools: Arc<StdMutex<ToolRegistry>>,
    /// Last activity timestamp for idle timeout detection (AJ-D4)
    pub last_activity: Arc<StdMutex<Instant>>,
    /// S4.T4: Session-lifetime cancellation token — thread-scoped interrupt.
    ///
    /// This token is ALSO stored in `AgentRuntime.session_interrupts` keyed
    /// by `SessionId`. It is a separate instance from the executor's
    /// per-turn `cancel_token` (which the executor resets on every
    /// `UserMessage`; see `executor.rs::run`). External state inspectors
    /// (tests, observability) can observe this flag to learn "session X
    /// was externally cancelled". For authoritative mid-turn interrupt
    /// dispatch, `AgentRuntime::cancel_session` also sends
    /// `AgentMessage::Cancel` through the handle — see D130.
    pub cancel_token: CancellationToken,
}

/// Idle-time distribution buckets for session monitoring (AM-T3).
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdleDistribution {
    /// Active sessions with idle time < 1 minute
    pub active_lt_1m: usize,
    /// Sessions idle 1–10 minutes
    pub idle_1m_to_10m: usize,
    /// Sessions idle 10 minutes–1 hour
    pub idle_10m_to_1h: usize,
    /// Sessions idle > 1 hour
    pub idle_gt_1h: usize,
}

/// Session monitoring metrics (AM-T3).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionMetrics {
    /// Number of currently active sessions
    pub active_count: usize,
    /// Maximum concurrent sessions allowed
    pub max_concurrent: usize,
    /// Total sessions ever created (from persistent store)
    pub total_created: usize,
    /// Distribution of active sessions by idle time
    pub idle_distribution: IdleDistribution,
    /// Average lifetime of active sessions in seconds
    pub avg_lifetime_secs: f64,
}

/// Session → AgentExecutorHandle 的注册表，同时持有所有共享运行时依赖
pub struct AgentRuntime {
    /// 单一主 executor（单用户场景）- 使用 Mutex 实现内部可变性
    pub(crate) primary_handle: Mutex<Option<AgentExecutorHandle>>,
    /// Phase AJ-T5: 多会话注册表 — SessionId → SessionEntry (Arc for sharing with session tools)
    pub(crate) sessions: Arc<DashMap<SessionId, SessionEntry>>,
    /// Phase AJ-T5: Primary session ID（兼容单用户模式）
    pub(crate) primary_session_id: Mutex<Option<SessionId>>,
    /// Phase AJ-T5: 最大并发会话数
    pub(crate) max_concurrent_sessions: usize,
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
    // Interaction gate for agent-to-user communication (Phase AS: InteractionGate wiring)
    pub(crate) interaction_gate: Arc<crate::tools::interaction::InteractionGate>,
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
    // Database connection for session registry persistence (AM-T5)
    pub(crate) db_conn: tokio_rusqlite::Connection,
    // Multi-agent task tracker (Phase AP-T12)
    pub(crate) task_tracker: Arc<TaskTracker>,
    // Multi-agent team manager (Phase AP-T13)
    pub(crate) team_manager: Arc<TeamManager>,
    // Plan mode buffer for enter_plan_mode/exit_plan_mode tools (T-G4)
    pub(crate) plan_buffer: crate::tools::plan_mode::PlanBuffer,
    // Autonomous session scheduler (Phase AU-G2)
    pub(crate) autonomous_scheduler: super::autonomous_scheduler::AutonomousScheduler,
    // Provider × model capability matrix (static baseline + cached probe
    // results). Populated at runtime startup (strategy = Eager) or on
    // first use (Lazy). harness queries it before arming features like
    // tool_choice=Required for D87 continuation.
    pub(crate) capability_store: Arc<crate::providers::CapabilityStore>,
    // S3.T5 (G7): per-session Stop hooks registered by runtime wrappers
    // (notably `grid-runtime::GridHarness` for EAASP scoped Stop hooks).
    // Keyed by `SessionId`; drained into the executor at spawn time by
    // `build_and_spawn_executor_filtered`. `DashMap` mirrors the
    // concurrency pattern used by `sessions` / `agent_handles` so
    // non-async call sites (the executor builder is sync) can mutate
    // without an `.await`.
    pub(crate) session_stop_hooks:
        DashMap<SessionId, Vec<Arc<dyn super::stop_hooks::StopHook>>>,
    /// S4.T4: Per-session cancellation registry — thread-scoped interrupt.
    ///
    /// Populated at session spawn in `start_session_full`, fired by
    /// `cancel_session(sid)`, cleared at `stop_session`. Multi-session
    /// isolation is guaranteed by `CancellationToken`'s per-instance
    /// `Arc<AtomicBool>`: cancelling session A does not affect session B.
    /// See `docs/design/EAASP/AGENT_LOOP_PATTERNS_TO_ADOPT.md` #10.
    pub(crate) session_interrupts: SessionInterruptRegistry,
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
        let _rt_start = std::time::Instant::now();
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
            if let Ok(password) = std::env::var("GRID_VAULT_PASSWORD") {
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
            // Wire credentials.yaml from GridRoot (populated by `octo auth login`)
            if let Some(ref root) = config.grid_root {
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
            crate::scheduler::SqliteSchedulerStorage::new(conn.clone()),
        );
        register_scheduler_tools(&mut tools, scheduler_storage);

        // 8. Create and load SkillRegistry

        let skill_registry = Arc::new(SkillRegistry::new());
        // Determine skills loading paths from GridRoot (if available) or legacy config
        let should_load_skills = config.grid_root.is_some() || !config.skills_dirs.is_empty();
        if should_load_skills {
            // Resolve project_dir and home_dir from GridRoot or fallback
            let (project_dir, home_dir) = if let Some(ref root) = config.grid_root {
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

            // Sync builtin skills to global ~/.grid/skills/ (never overwrites existing)
            let global_skills_dir = if let Some(ref root) = config.grid_root {
                root.global_skills_dir()
            } else {
                home_dir
                    .as_ref()
                    .map(|h| h.join(".grid").join("skills"))
                    .unwrap_or_else(|| PathBuf::from(".grid/skills"))
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

        let (mcp_manager, deferred_mcp_configs) = {
            let mgr = McpManager::new();

            // Collect config paths to load (highest priority first):
            // 1. $PROJECT/.grid/mcp.json    (octo-native, project-level)
            // 2. $PROJECT/.mcp.json         (CC-compatible, project-level)
            // 3. ~/.grid/mcp/mcp.json       (octo-native, global)
            let mut config_paths = Vec::new();
            if let Some(ref root) = config.grid_root {
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
            let mut deferred_configs: Vec<crate::mcp::traits::McpServerConfig> = Vec::new();
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
                            // All servers connect in background for fast startup.
                            // autoStart=true servers are prioritized but still non-blocking.
                            loaded_names.insert(server_config.name.clone());
                            deferred_configs.push(server_config);
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
                    "MCP servers will connect in background"
                );
            }

            (Arc::new(Mutex::new(mgr)), deferred_configs)
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

        // 17b. InteractionGate for agent-to-user communication (Phase AS)
        let interaction_gate = Arc::new(crate::tools::interaction::InteractionGate::new());

        // 18. (CredentialResolver moved to step 5b — before provider creation)

        // 19. SessionSandboxManager (SSM) — conditional on run mode + profile
        let session_sandbox: Option<Arc<SessionSandboxManager>> = {
            let run_mode = GridRunMode::detect();
            let profile = SandboxProfile::resolve(
                false,
                config.sandbox_profile.as_deref(),
                None,
            );
            if run_mode == GridRunMode::Host && profile != SandboxProfile::Development {
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
            sessions: Arc::new(DashMap::new()),
            primary_session_id: Mutex::new(None),
            max_concurrent_sessions: config.max_concurrent_sessions.unwrap_or(DEFAULT_MAX_CONCURRENT_SESSIONS),
            agent_handles: DashMap::new(),
            session_stop_hooks: DashMap::new(),
            session_interrupts: SessionInterruptRegistry::new(),
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

                            // Discover WASM plugins from global + project directories
                            #[cfg(feature = "sandbox-wasm")]
                            let wasm_handlers = {
                                let mut plugin_dirs = Vec::new();
                                // Global: ~/.grid/plugins/
                                if let Some(home) = dirs::home_dir() {
                                    plugin_dirs.push(home.join(".grid").join("plugins"));
                                }
                                // Project: $WORKING_DIR/.grid/plugins/
                                plugin_dirs.push(wd.join(".grid").join("plugins"));

                                let discovered = crate::hooks::wasm::loader::discover_plugins(&plugin_dirs);
                                let engine = crate::hooks::wasm::handler::WasmHookHandler::create_engine()
                                    .unwrap_or_else(|e| {
                                        tracing::warn!(error = %e, "Failed to create WASM engine with fuel, using default");
                                        wasmtime::Engine::default()
                                    });
                                let mut handlers = std::collections::HashMap::new();
                                for plugin in discovered {
                                    match crate::hooks::wasm::handler::WasmHookHandler::load(
                                        &engine,
                                        plugin.manifest.clone(),
                                        &plugin.wasm_path,
                                    ) {
                                        Ok(handler) => {
                                            let name = plugin.manifest.name.clone();
                                            tracing::info!(plugin = %name, "Loaded WASM hook plugin");
                                            handlers.insert(name, std::sync::Arc::new(handler));
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                plugin = %plugin.manifest.name,
                                                error = %e,
                                                "Failed to load WASM hook plugin"
                                            );
                                        }
                                    }
                                }
                                handlers
                            };

                            // Register DeclarativeHookBridge for each configured hook point
                            let hook_points = collect_declarative_hook_points(&hc);
                            for hp in hook_points {
                                let bridge = crate::hooks::declarative::DeclarativeHookBridge::new(hc.clone(), hp);
                                // Wire WASM handlers into bridge
                                #[cfg(feature = "sandbox-wasm")]
                                for (name, handler) in &wasm_handlers {
                                    bridge.register_wasm_handler(name.clone(), handler.clone());
                                }
                                r.register(hp, Arc::new(bridge)).await;
                            }

                            #[cfg(feature = "sandbox-wasm")]
                            if !wasm_handlers.is_empty() {
                                tracing::info!(count = wasm_handlers.len(), "WASM hook plugins registered");
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
            interaction_gate: interaction_gate.clone(),
            collaboration_manager: None,
            knowledge_graph,
            credential_resolver,
            session_sandbox,
            session_summary_store,
            db_conn: conn,
            task_tracker: Arc::new(TaskTracker::new()),
            team_manager: Arc::new(TeamManager::new()),
            plan_buffer: crate::tools::plan_mode::PlanBuffer::new(),
            autonomous_scheduler: super::autonomous_scheduler::AutonomousScheduler::new(),
            capability_store: Arc::new(crate::providers::CapabilityStore::new()),
        };


        // 16.5. Register built-in agents (before YAML so YAML can override)
        let builtin_count = crate::agent::builtin_agents::register_builtin_agents(&runtime.catalog);
        tracing::info!(count = builtin_count, "Registered built-in agents");

        // 17. YAML agent loading removed (Phase AY).
        // Builtin agents are code-only. User-defined agents use Playbook skills.

        // 18. Register MCP management tools (mcp_install, mcp_remove, mcp_list)
        {
            let mcp_config_path = config
                .grid_root
                .as_ref()
                .map(|r| r.project_root().join("mcp.json"))
                .unwrap_or_else(|| PathBuf::from(".grid/mcp.json"));
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

        // 18b. Register multi-agent coordination tools (Phase AP Wave 5)
        {
            let mut tools_guard = runtime.tools.lock().unwrap_or_else(|e| e.into_inner());
            // Task management tools
            tools_guard.register(crate::tools::task::TaskCreateTool::new(runtime.task_tracker.clone()));
            tools_guard.register(crate::tools::task::TaskUpdateTool::new(runtime.task_tracker.clone()));
            tools_guard.register(crate::tools::task::TaskListTool::new(runtime.task_tracker.clone()));
            // Team management tools
            tools_guard.register(crate::tools::team::TeamCreateTool::new(runtime.team_manager.clone()));
            tools_guard.register(crate::tools::team::TeamAddMemberTool::new(runtime.team_manager.clone()));
            tools_guard.register(crate::tools::team::TeamDissolveTool::new(runtime.team_manager.clone()));
            // Phase AS: InteractionGate + AskUserTool + ToolSearchTool wiring
            tools_guard.register(crate::tools::ask_user::AskUserTool::new(runtime.interaction_gate.clone()));
            tools_guard.register(crate::tools::tool_search::ToolSearchTool::new(runtime.tools.clone()));
            // T-G4: Plan mode tools
            tools_guard.register(crate::tools::plan_mode::EnterPlanModeTool::new(runtime.plan_buffer.clone()));
            tools_guard.register(crate::tools::plan_mode::ExitPlanModeTool::new(runtime.plan_buffer.clone()));
            // T-G1: Session management tools (message/status/stop registered here;
            // session_create registered post-init via register_session_create_tool)
            tools_guard.register(crate::tools::session::SessionMessageTool::new(runtime.sessions.clone()));
            tools_guard.register(crate::tools::session::SessionStatusTool::new(runtime.sessions.clone()));
            tools_guard.register(crate::tools::session::SessionStopTool::new(runtime.sessions.clone()));
        }

        // 19. Spawn background tasks for MCP servers (parallel, non-blocking)
        // All servers now connect in background for instant startup.
        if !deferred_mcp_configs.is_empty() {
            let count = deferred_mcp_configs.len();
            tracing::info!(count, "Spawning parallel background connections for MCP servers");
            for server_config in deferred_mcp_configs {
                let mcp_mgr = runtime.mcp_manager.clone();
                let tools_registry = runtime.tools.clone();
                let name = server_config.name.clone();
                tokio::spawn(async move {
                    tracing::debug!(server = %name, "Background: connecting MCP server");

                    // Mark as starting
                    {
                        let mut guard = mcp_mgr.lock().await;
                        guard.set_runtime_state(
                            &name,
                            crate::mcp::manager::ServerRuntimeState::Starting,
                        );
                    }

                    // Connect outside the lock
                    let mut client = StdioMcpClient::new(server_config);
                    match client.connect().await {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::warn!(
                                server = %name,
                                error = %e,
                                "Background: failed to connect MCP server"
                            );
                            let mut guard = mcp_mgr.lock().await;
                            guard.set_runtime_state(
                                &name,
                                crate::mcp::manager::ServerRuntimeState::Error {
                                    message: e.to_string(),
                                },
                            );
                            return;
                        }
                    }

                    // Discover tools
                    let tools: Vec<crate::mcp::traits::McpToolInfo> = match client.list_tools().await {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(
                                server = %name,
                                error = %e,
                                "Background: failed to list tools from MCP server"
                            );
                            let mut guard = mcp_mgr.lock().await;
                            guard.set_runtime_state(
                                &name,
                                crate::mcp::manager::ServerRuntimeState::Error {
                                    message: e.to_string(),
                                },
                            );
                            return;
                        }
                    };

                    tracing::info!(
                        server = %name,
                        tool_count = tools.len(),
                        "Background: MCP server connected"
                    );

                    let client_arc: Arc<tokio::sync::RwLock<Box<dyn crate::mcp::traits::McpClient>>> =
                        Arc::new(tokio::sync::RwLock::new(Box::new(client)));

                    // Insert into manager
                    {
                        let mut guard = mcp_mgr.lock().await;
                        guard.insert_connected_client(name.clone(), client_arc.clone(), tools.clone());
                    }

                    // Register tool bridges
                    {
                        let mut tools_guard = tools_registry.lock().unwrap_or_else(|e| e.into_inner());
                        for tool_info in &tools {
                            let bridge = crate::mcp::bridge::McpToolBridge::new(
                                client_arc.clone(),
                                name.clone(),
                                tool_info.clone(),
                            );
                            tools_guard.register(bridge);
                        }
                    }
                });
            }
        }

        tracing::info!("AgentRuntime::new() completed in {:?}", _rt_start.elapsed());
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

    /// Shared capability matrix. Harness looks up (provider, model, base_url)
    /// to decide whether to arm provider-specific features like
    /// `tool_choice=Required` at runtime.
    pub fn capability_store(&self) -> &Arc<crate::providers::CapabilityStore> {
        &self.capability_store
    }

    pub fn mcp_manager(&self) -> &Arc<Mutex<crate::mcp::manager::McpManager>> {
        &self.mcp_manager
    }

    /// Get the multi-agent task tracker (Phase AP-T12).
    pub fn task_tracker(&self) -> &Arc<TaskTracker> {
        &self.task_tracker
    }

    /// Get the multi-agent team manager (Phase AP-T13).
    pub fn team_manager(&self) -> &Arc<TeamManager> {
        &self.team_manager
    }

    /// Get autonomous session scheduler (Phase AU-G2).
    pub fn autonomous_scheduler(&self) -> &super::autonomous_scheduler::AutonomousScheduler {
        &self.autonomous_scheduler
    }

    /// Get metering snapshot for observability
    pub fn metering(&self) -> crate::metering::MeteringSnapshot {
        self.metering.snapshot()
    }

    /// Get raw metering Arc for reset operations
    pub fn metering_arc(&self) -> &Arc<Metering> {
        &self.metering
    }

    /// Get security policy
    pub fn security_policy(&self) -> &Arc<SecurityPolicy> {
        &self.security_policy
    }

    /// Get hook registry
    pub fn hook_registry(&self) -> &Arc<HookRegistry> {
        &self.hook_registry
    }

    /// Get telemetry event bus (if enabled)
    pub fn event_bus(&self) -> Option<&Arc<TelemetryBus>> {
        self.event_bus.as_ref()
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

    /// Get session sandbox manager (if initialized)
    pub fn session_sandbox_manager(&self) -> Option<&Arc<crate::sandbox::SessionSandboxManager>> {
        self.session_sandbox.as_ref()
    }

    /// Get shared approval gate (if any) — T7
    pub fn approval_gate(&self) -> Option<&crate::tools::approval::ApprovalGate> {
        self.approval_gate.as_ref()
    }

    /// Get shared interaction gate for agent-to-user communication (Phase AS)
    pub fn interaction_gate(&self) -> &Arc<crate::tools::interaction::InteractionGate> {
        &self.interaction_gate
    }

    /// Register session_create tool post-construction (needs Arc<Self>).
    /// Call this once after wrapping AgentRuntime in Arc.
    pub fn register_session_create_tool(self: &Arc<Self>) {
        let mut tools_guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools_guard.register(crate::tools::session::SessionCreateTool::new(self.clone()));
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

    // ── Phase AJ-T5: Multi-session registry getters ─────────────────────

    /// Get session handle by session ID
    pub fn get_session_handle(&self, session_id: &SessionId) -> Option<AgentExecutorHandle> {
        self.sessions.get(session_id).map(|e| e.handle.clone())
    }

    /// Returns a clone of the cancel token for a given session.
    ///
    /// **Intended consumers**: future REST/gRPC interrupt endpoints that need to
    /// observe cancellation state (e.g. return 409 if already cancelled). For
    /// dispatching a cancel, use [`Self::cancel_session`] instead — this accessor
    /// is read-only.
    ///
    /// Hidden from public docs pending stabilization of the interrupt API
    /// surface (see D130 for the planned consolidation).
    #[doc(hidden)]
    pub fn get_session_cancel_token(&self, session_id: &SessionId) -> Option<CancellationToken> {
        self.sessions
            .get(session_id)
            .map(|e| e.cancel_token.clone())
    }

    /// List all active session IDs
    pub fn active_sessions(&self) -> Vec<SessionId> {
        self.sessions.iter().map(|e| e.key().clone()).collect()
    }

    /// Number of active sessions
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Maximum concurrent sessions allowed
    pub fn max_concurrent_sessions(&self) -> usize {
        self.max_concurrent_sessions
    }

    /// Update last_activity timestamp for a session (AJ-D4).
    /// Called by WS handler on each incoming message to track activity.
    pub fn touch_session(&self, session_id: &SessionId) {
        if let Some(entry) = self.sessions.get(session_id) {
            let mut guard = entry.last_activity.lock().unwrap_or_else(|e| e.into_inner());
            *guard = Instant::now();
        }
    }

    /// Clean up sessions that have been idle longer than `timeout`.
    /// Returns the number of sessions recycled. Skips the primary session.
    pub async fn cleanup_idle_sessions(&self, timeout: std::time::Duration) -> usize {
        let primary_sid = self.primary_session_id.lock().await.clone();
        let now = Instant::now();

        // Collect expired (non-primary) session IDs
        let expired: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|entry| {
                // Never recycle primary session
                if primary_sid.as_ref() == Some(entry.key()) {
                    return false;
                }
                let last = entry.last_activity.lock().unwrap_or_else(|e| e.into_inner());
                now.duration_since(*last) > timeout
            })
            .map(|entry| entry.key().clone())
            .collect();

        let count = expired.len();
        for sid in expired {
            info!(session_id = %sid.as_str(), "Recycling idle session (timeout exceeded)");
            // AM-T5: persist idle_recycled status before stopping
            self.persist_session_stop(&sid, "idle_recycled").await;
            // Remove from in-memory registry (skip the default 'stopped' persist in stop_session
            // since we already set 'idle_recycled')
            let removed = self.sessions.remove(&sid);
            if removed.is_some() {
                let mut primary_guard = self.primary_session_id.lock().await;
                if primary_guard.as_ref() == Some(&sid) {
                    *primary_guard = None;
                    let mut handle_guard = self.primary_handle.lock().await;
                    *handle_guard = None;
                }
                {
                    let mut mcp_guard = self.mcp_manager.lock().await;
                    mcp_guard.cleanup_session(sid.as_str());
                }
            }
        }
        count
    }

    // ── AM-T5: Session registry persistence (crash recovery) ────────────

    /// Persist a session entry to the session_registry table.
    async fn persist_session_start(
        &self,
        session_id: &SessionId,
        user_id: &UserId,
        agent_id: Option<&AgentId>,
        sandbox_id: &SandboxId,
    ) {
        let sid = session_id.as_str().to_string();
        let uid = user_id.as_str().to_string();
        let aid = agent_id.map(|a| a.0.clone());
        let sbid = sandbox_id.as_str().to_string();
        if let Err(e) = self
            .db_conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO session_registry (session_id, user_id, agent_id, status, sandbox_id) \
                     VALUES (?1, ?2, ?3, 'running', ?4)",
                    rusqlite::params![sid, uid, aid, sbid],
                )?;
                Ok(())
            })
            .await
        {
            tracing::warn!(error = %e, "Failed to persist session start to registry");
        }
    }

    /// Update session_registry status for a stopped session.
    async fn persist_session_stop(&self, session_id: &SessionId, status: &str) {
        let sid = session_id.as_str().to_string();
        let st = status.to_string();
        if let Err(e) = self
            .db_conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE session_registry SET status = ?1 WHERE session_id = ?2",
                    rusqlite::params![st, sid],
                )?;
                Ok(())
            })
            .await
        {
            tracing::warn!(error = %e, "Failed to persist session stop to registry");
        }
    }

    /// Called on startup: detect sessions left in 'running' state from a previous run
    /// and mark them as 'crashed'. Does NOT restore actual executors.
    pub async fn restore_sessions(&self) -> usize {
        let result = self
            .db_conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT session_id FROM session_registry WHERE status = 'running'",
                )?;
                let ids: Vec<String> = stmt
                    .query_map([], |row| row.get::<_, String>(0))?
                    .filter_map(|r| r.ok())
                    .collect();

                if !ids.is_empty() {
                    conn.execute(
                        "UPDATE session_registry SET status = 'crashed' WHERE status = 'running'",
                        [],
                    )?;
                }
                Ok(ids.len())
            })
            .await;

        match result {
            Ok(count) => {
                if count > 0 {
                    tracing::warn!(
                        count,
                        "Found {} session(s) from previous run marked as crashed",
                        count
                    );
                }
                count
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to restore sessions from registry");
                0
            }
        }
    }

    /// Called on graceful shutdown: mark all currently running sessions as 'shutting_down'.
    pub async fn save_session_state(&self) {
        let active_ids: Vec<String> = self
            .sessions
            .iter()
            .map(|e| e.key().as_str().to_string())
            .collect();

        if active_ids.is_empty() {
            return;
        }

        let count = active_ids.len();
        if let Err(e) = self
            .db_conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                for sid in &active_ids {
                    tx.execute(
                        "UPDATE session_registry SET status = 'shutting_down' WHERE session_id = ?1",
                        rusqlite::params![sid],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
        {
            tracing::warn!(error = %e, "Failed to save session state on shutdown");
        } else {
            tracing::info!(count, "Session state saved (marked as shutting_down)");
        }
    }

    /// Compute session monitoring metrics (AM-T3).
    ///
    /// Returns a struct with active count, max concurrent, total created,
    /// idle-time distribution buckets, and average lifetime in seconds.
    pub async fn session_metrics(&self) -> SessionMetrics {
        let active_count = self.sessions.len();
        let max_concurrent = self.max_concurrent_sessions;

        // Total sessions ever created (from persistent store)
        let total_created = self.session_store.count_all_sessions().await;

        // Idle distribution — classify active sessions by elapsed since last_activity
        let now = Instant::now();
        let mut active_lt_1m: usize = 0;
        let mut idle_1m_to_10m: usize = 0;
        let mut idle_10m_to_1h: usize = 0;
        let mut idle_gt_1h: usize = 0;

        let mut lifetime_sum_secs: f64 = 0.0;

        for entry in self.sessions.iter() {
            let last = entry.last_activity.lock().unwrap_or_else(|e| e.into_inner());
            let idle = now.duration_since(*last);
            drop(last);

            if idle.as_secs() < 60 {
                active_lt_1m += 1;
            } else if idle.as_secs() < 600 {
                idle_1m_to_10m += 1;
            } else if idle.as_secs() < 3600 {
                idle_10m_to_1h += 1;
            } else {
                idle_gt_1h += 1;
            }

            // Lifetime = time since session was created
            lifetime_sum_secs += now.duration_since(entry.created_at).as_secs_f64();
        }

        let avg_lifetime_secs = if active_count > 0 {
            lifetime_sum_secs / active_count as f64
        } else {
            0.0
        };

        SessionMetrics {
            active_count,
            max_concurrent,
            total_created,
            idle_distribution: IdleDistribution {
                active_lt_1m,
                idle_1m_to_10m,
                idle_10m_to_1h,
                idle_gt_1h,
            },
            avg_lifetime_secs,
        }
    }

    /// Get primary session ID (if set)
    pub async fn primary_session_id(&self) -> Option<SessionId> {
        self.primary_session_id.lock().await.clone()
    }

    /// 获取主 AgentExecutorHandle（如果已启动）
    pub async fn primary(&self) -> Option<AgentExecutorHandle> {
        let guard = self.primary_handle.lock().await;
        guard.clone()
    }

    // ── Phase AJ-T6: Multi-session lifecycle API ───────────────────────

    /// Build a session-scoped executor with isolated tools, KG, and memory,
    /// spawn it as a Tokio task, and return its handle + session tools.
    /// This is the shared builder used by both `start_session()` and `start_primary()`.
    fn build_and_spawn_executor(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
    ) -> (AgentExecutorHandle, Arc<StdMutex<ToolRegistry>>, CancellationTokenTree) {
        self.build_and_spawn_executor_filtered(session_id, user_id, sandbox_id, initial_history, agent_id, None)
    }

    /// Like `build_and_spawn_executor` but with optional tool filter.
    /// When `tool_filter` is Some, only the named tools are included in the
    /// session snapshot (EAASP skill allowed-tools enforcement).
    fn build_and_spawn_executor_filtered(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
        tool_filter: Option<&[String]>,
    ) -> (AgentExecutorHandle, Arc<StdMutex<ToolRegistry>>, CancellationTokenTree) {
        // 从 manifest 解析运行时配置（不含 tools，使用全局共享引用）
        let (_, system_prompt, model, config) = self.resolve_runtime_config(agent_id);

        let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

        let handle = AgentExecutorHandle {
            tx,
            broadcast_tx: broadcast_tx.clone(),
            session_id: session_id.clone(),
        };

        // Phase AJ-T1: 创建 session 级 ToolRegistry 快照（隔离：session A 的 MCP 安装不影响 session B）
        // When tool_filter is provided (EAASP skill), only include those tools.
        let session_tools = {
            let guard = self.tools.lock().unwrap_or_else(|e| e.into_inner());
            let base_snapshot = match tool_filter {
                Some(filter) => guard.snapshot_filtered(filter),
                None => guard.snapshot(),
            };
            let session_reg = Arc::new(StdMutex::new(base_snapshot));

            // 重建 MCP 管理工具，指向 session 级 registry（而非全局）
            let mcp_config_path = self.working_dir.join(".grid/mcp.json");
            let session_mcp_handle = crate::tools::mcp_manage::McpManageHandle {
                mcp_manager: self.mcp_manager.clone(),
                tools: session_reg.clone(),
                config_path: mcp_config_path,
            };
            // AJ-T2: 创建 session 级 KnowledgeGraph 实例
            let session_kg = Arc::new(tokio::sync::RwLock::new(crate::memory::KnowledgeGraph::new()));

            {
                let mut reg = session_reg.lock().unwrap_or_else(|e| e.into_inner());
                // Session-scoped MCP manage tools and KG tools must respect
                // `tool_filter`. When a skill declares a filter (EAASP
                // skill session), these "workbench" tools are typically not
                // part of the workflow — registering them would expose
                // `graph_add/query/relate`, `mcp_install`, etc. to the LLM
                // and cause the model to wander into tool loops that never
                // converge. When `tool_filter` is `None` (free-form Grid
                // workbench session) these tools are always registered.
                let allow_tool = |name: &str| -> bool {
                    match tool_filter {
                        Some(filter) => filter.iter().any(|f| f == name),
                        None => true,
                    }
                };
                if allow_tool("mcp_install") {
                    reg.register(crate::tools::mcp_manage::McpInstallTool::new(session_mcp_handle.clone()));
                }
                if allow_tool("mcp_remove") {
                    reg.register(crate::tools::mcp_manage::McpRemoveTool::new(session_mcp_handle.clone()));
                }
                if allow_tool("mcp_list") {
                    reg.register(crate::tools::mcp_manage::McpListTool::new(session_mcp_handle));
                }
                // KG tools (graph_add/query/relate). When filter is set and
                // does not include any of them, skip the block entirely.
                if tool_filter.map_or(true, |f| {
                    f.iter()
                        .any(|t| t == "graph_add" || t == "graph_query" || t == "graph_relate")
                }) {
                    register_kg_tools(&mut reg, session_kg.clone());
                }
            }
            session_reg
        };

        let mut executor = AgentExecutor::new(
            session_id.clone(),
            user_id,
            sandbox_id,
            initial_history,
            rx,
            broadcast_tx,
            self.provider.clone(),
            session_tools.clone(),
            Arc::new(InMemoryWorkingMemory::new()),
            Some(self.memory_store.clone()),
            Some(model.clone()),
            Some(self.session_store.clone()),
            system_prompt,
            config,
            self.working_dir.clone(),
            self.event_bus.clone(),
            Some(self.security_policy.clone() as Arc<dyn grid_types::PathValidator>),
            Some(self.hook_registry.clone()),
            self.safety_pipeline.clone(),
            self.canary_token.clone(),
            self.approval_gate.clone(),
            self.skill_registry.clone(),
            Some(self.recorder.clone()),
            self.session_sandbox.clone(),
            self.session_summary_store.clone(),
            self.interaction_gate.clone(),
            Some(self.catalog.clone()),
        );

        // D87 Fix 2 (L2b): consult the capability matrix and forward
        // tool_choice support into the executor. This decides whether
        // the harness will arm `force_tool_choice_next_call` on
        // workflow-continuation triggers.
        //
        // base_url is unknown at this layer (the provider trait doesn't
        // expose it); we use an empty string. Eager probes from
        // grid-runtime startup pre-populate the cache by full key, but
        // the static baseline accepts empty base_url for OpenAI/Anthropic
        // direct, so this fallback covers the common case.
        let cap_key = crate::providers::CapabilityKey::new(
            self.provider.id(),
            &model,
            "",
        );
        let tool_choice_cap = self.capability_store.get(&cap_key).tool_choice;
        executor.set_tool_choice_supported(
            tool_choice_cap == crate::providers::Capability::Supported,
        );

        // D130: create the session/turn token tree and wire it into the executor.
        // The session_cancellation_token() is returned to the caller so it can
        // be registered in SessionInterruptRegistry.
        let cancel_tree = CancellationTokenTree::new();
        executor.set_cancel_tree(cancel_tree.clone());

        // S3.T5 (G7): drain any Stop hooks registered for this session by
        // a runtime wrapper (GridHarness → scoped Stop hooks from skill
        // frontmatter). Must happen AFTER `AgentExecutor::new` but BEFORE
        // the executor's spawn loop so the hooks land in the first
        // `AgentLoopConfig` built on UserMessage. Using `remove` (rather
        // than a clone) ensures the map never grows unbounded across
        // session churn — each SessionId is consumed exactly once.
        if let Some((_, stop_hooks)) = self.session_stop_hooks.remove(&session_id) {
            if !stop_hooks.is_empty() {
                tracing::info!(
                    session_id = %session_id,
                    count = stop_hooks.len(),
                    "Forwarding scoped Stop hooks into AgentExecutor"
                );
                executor.set_stop_hooks(stop_hooks);
            }
        }

        // Spawn 持久化主循环
        tokio::spawn(async move {
            executor.run().await;
        });

        (handle, session_tools, cancel_tree)
    }

    /// S3.T5 (G7): register Stop hooks for a session.
    ///
    /// Called by runtime wrappers (notably `grid-runtime::GridHarness`)
    /// **before** invoking `start_session_*`. Hooks are consumed once by
    /// `build_and_spawn_executor_filtered` when the executor is spawned;
    /// subsequent calls for the same `SessionId` are additive until the
    /// executor spawn drains them.
    ///
    /// This API accepts `Vec<Arc<dyn StopHook>>` so callers can mix
    /// concrete types (e.g. `ScopedStopHookBridge` for bash hooks +
    /// future native Rust `StopHook` impls) in a single registration.
    /// Passing an empty vec is a no-op.
    pub fn register_session_stop_hooks(
        &self,
        session_id: &SessionId,
        hooks: Vec<Arc<dyn super::stop_hooks::StopHook>>,
    ) {
        if hooks.is_empty() {
            return;
        }
        // Merge with any previously registered hooks for the same
        // session (idempotent-friendly: harness may register multiple
        // hook groups across initialization phases).
        self.session_stop_hooks
            .entry(session_id.clone())
            .or_insert_with(Vec::new)
            .extend(hooks);
    }

    /// 创建并启动新会话（Phase AJ-T6）
    ///
    /// 构建独立 executor（独立 channels、WorkingMemory、KG、session_tools），
    /// 注册到 sessions DashMap，返回 handle。
    pub async fn start_session(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        self.start_session_with_tool_filter(session_id, user_id, sandbox_id, initial_history, agent_id, None).await
    }

    /// Start a session with an optional tool filter (EAASP skill allowed-tools).
    /// When `tool_filter` is Some, only the named tools are exposed to the agent.
    pub async fn start_session_with_tool_filter(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
        tool_filter: Option<&[String]>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        self.start_session_full(session_id, user_id, sandbox_id, initial_history, agent_id, None, tool_filter).await
    }

    /// Start a new session with optional autonomous mode (AU-D1).
    pub async fn start_session_with_autonomous(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
        autonomous: Option<super::autonomous::AutonomousConfig>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        self.start_session_full(session_id, user_id, sandbox_id, initial_history, agent_id, autonomous, None).await
    }

    /// Full session start with all options.
    async fn start_session_full(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        agent_id: Option<&AgentId>,
        autonomous: Option<super::autonomous::AutonomousConfig>,
        tool_filter: Option<&[String]>,
    ) -> Result<AgentExecutorHandle, AgentError> {
        // Check if session already exists
        if let Some(entry) = self.sessions.get(&session_id) {
            return Ok(entry.handle.clone());
        }

        // Check concurrent session limit
        if self.sessions.len() >= self.max_concurrent_sessions {
            return Err(AgentError::Internal(format!(
                "Maximum concurrent sessions reached ({})",
                self.max_concurrent_sessions
            )));
        }

        // AM-T5: capture sandbox_id for registry persistence before move
        let sandbox_id_for_registry = sandbox_id.clone();

        let (handle, session_tools, cancel_tree) = self.build_and_spawn_executor_filtered(
            session_id.clone(),
            user_id.clone(),
            sandbox_id,
            initial_history,
            agent_id,
            tool_filter,
        );

        if let Some(id) = agent_id {
            let cancel_token = CancellationToken::new();
            self.agent_handles.insert(id.clone(), cancel_token);
            self.catalog.update_state(id, AgentStatus::Running);
        }

        // AM-T5: persist to session_registry for crash recovery
        self.persist_session_start(&session_id, &user_id, agent_id, &sandbox_id_for_registry).await;

        // Register in session registry
        let now = Instant::now();
        // D130: register the session-lifetime token from the tree so that
        // SessionInterruptRegistry::cancel() propagates into all active and
        // future per-turn tokens via the shared AtomicBool flag.
        let session_cancel_token = cancel_tree.session_cancellation_token();
        self.session_interrupts
            .register(session_id.clone(), session_cancel_token.clone());
        self.sessions.insert(
            session_id.clone(),
            SessionEntry {
                handle: handle.clone(),
                user_id,
                created_at: now,
                tools: session_tools,
                last_activity: Arc::new(StdMutex::new(now)),
                cancel_token: session_cancel_token,
            },
        );

        // AU-D1: Register with AutonomousScheduler if autonomous mode enabled
        if let Some(auto_config) = autonomous {
            let auto_state = super::autonomous::AutonomousState::new(
                session_id.clone(),
                auto_config,
            );
            self.autonomous_scheduler.register(auto_state);
            info!(session_id = %session_id.as_str(), "Session started with autonomous mode");
        } else {
            info!(session_id = %session_id.as_str(), "Session started (registered in multi-session registry)");
        }

        Ok(handle)
    }

    /// 停止并清理会话（Phase AJ-T6）
    ///
    /// 从 sessions DashMap 移除 + drop handle → tx dropped → executor 自然退出。
    /// 同时清理 MCP server 所有权。
    pub async fn stop_session(&self, session_id: &SessionId) {
        let removed = self.sessions.remove(session_id);
        if removed.is_some() {
            // Also clear primary_session_id if this was the primary
            let mut primary_guard = self.primary_session_id.lock().await;
            if primary_guard.as_ref() == Some(session_id) {
                *primary_guard = None;
                // Also clear legacy primary_handle
                let mut handle_guard = self.primary_handle.lock().await;
                *handle_guard = None;
            }

            // Clean up MCP server ownership for this session
            {
                let mut mcp_guard = self.mcp_manager.lock().await;
                mcp_guard.cleanup_session(session_id.as_str());
            }

            // S4.T4: drop the thread-scoped interrupt entry so the registry
            // never grows unbounded across session churn.
            self.session_interrupts.remove(session_id);

            // AM-T5: persist stop status to session_registry
            self.persist_session_stop(session_id, "stopped").await;

            info!(session_id = %session_id.as_str(), "Session stopped and cleaned up");
        }
    }

    /// S4.T4: Fire thread-scoped interrupt for a specific session.
    ///
    /// Returns `true` if the session was registered and its
    /// cancellation path was dispatched. Returns `false` if the session
    /// is unknown (may have already exited naturally — not an error).
    ///
    /// # Isolation
    /// Only the target session's cancel path fires. Other sessions'
    /// executors are unaffected. This is the authoritative external
    /// mid-call interrupt entry point; `stop_session` is the graceful
    /// channel-close path that drops the handle.
    ///
    /// # Dispatch
    /// Fires two paths belt-and-suspenders style:
    /// 1. The session-lifetime token registered in
    ///    `session_interrupts` — observable to tests and external
    ///    inspectors.
    /// 2. `AgentMessage::Cancel` via the handle — the executor's
    ///    internal path that flips the per-turn `cancel_token` currently
    ///    watched by the harness `run_agent_loop`. Only this path
    ///    interrupts an in-flight turn because the executor resets its
    ///    `cancel_token` on each `UserMessage` (see `executor.rs::run`).
    ///
    /// A structural consolidation (unified session-lifetime token that
    /// the executor also watches directly) is tracked as D130.
    pub async fn cancel_session(&self, session_id: &SessionId) -> bool {
        // Path 1: flip the session-lifetime token so external observers
        // see the cancellation even if the executor has exited already.
        let fired = self.session_interrupts.cancel(session_id);

        // Path 2: if the session is live, dispatch AgentMessage::Cancel
        // through the executor handle so the currently running turn is
        // interrupted. A closed channel means the executor already exited,
        // which we treat as a successful no-op on top of path 1.
        //
        // S4.T4/D131: clone the handle out of the DashMap guard BEFORE
        // awaiting, mirroring the idiom at runtime_lifecycle.rs:55-63 —
        // holding a DashMap Ref across .await is a deadlock risk per
        // dashmap docs. S4.T4/N2: log send errors at debug! level so
        // diagnostic signal is preserved without elevating "executor
        // already exited" to a real error.
        let handle = self
            .sessions
            .get(session_id)
            .map(|entry| entry.value().handle.clone());
        if let Some(h) = handle {
            if let Err(e) = h.send(AgentMessage::Cancel).await {
                tracing::debug!(
                    session_id = %session_id.as_str(),
                    error = %e,
                    "cancel_session: AgentMessage::Cancel send failed (executor likely already exited)"
                );
            }
        }

        if fired {
            tracing::info!(
                session_id = %session_id.as_str(),
                "Session cancelled via thread-scoped interrupt"
            );
        } else {
            tracing::debug!(
                session_id = %session_id.as_str(),
                "cancel_session called for unknown session (already exited?)"
            );
        }
        fired
    }

    /// S4.T4: Accessor for the session interrupt registry.
    ///
    /// Exposed for tests and runtime wrappers that need to inspect or
    /// share the registry. Clone is cheap (inner Arc<DashMap>).
    pub fn session_interrupts(&self) -> &SessionInterruptRegistry {
        &self.session_interrupts
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
        let mut handle_guard = self.primary_handle.lock().await;

        // Return existing handle if already started
        if let Some(ref handle) = *handle_guard {
            return handle.clone();
        }

        // Use start_session internally (Phase AJ-T7: reuse)
        let handle = match self
            .start_session(
                session_id.clone(),
                user_id,
                sandbox_id,
                initial_history,
                agent_id,
            )
            .await
        {
            Ok(h) => h,
            Err(e) => {
                // start_primary is infallible by contract — log and create minimal executor
                tracing::error!(error = %e, "start_session failed in start_primary, this should not happen");
                // Fallback: build directly (should never reach here in practice)
                let (h, _, _) = self.build_and_spawn_executor(
                    session_id.clone(),
                    UserId::from_string("fallback"),
                    SandboxId::from_string("default"),
                    vec![],
                    agent_id,
                );
                h
            }
        };

        // Mark as primary
        {
            let mut primary_guard = self.primary_session_id.lock().await;
            *primary_guard = Some(session_id.clone());
        }

        info!(session_id = %session_id.as_str(), "Primary AgentExecutor started");

        *handle_guard = Some(handle.clone());
        handle
    }

    /// 停止主 Runtime
    pub async fn stop_primary(&self) {
        // Get and clear primary session ID
        let primary_sid = {
            let mut guard = self.primary_session_id.lock().await;
            guard.take()
        };

        // Remove from sessions registry
        if let Some(sid) = &primary_sid {
            self.sessions.remove(sid);
        }

        // Clear legacy primary_handle
        let _dropped_handle = {
            let mut guard = self.primary_handle.lock().await;
            guard.take()
        };
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
    if let Ok(env_path) = std::env::var("GRID_POLICIES_FILE") {
        let p = std::path::PathBuf::from(&env_path);
        if p.exists() {
            return Some(p);
        }
    }
    // 2. Project-level config
    if let Some(project) = project_dir {
        let project_policies = project.join(".grid").join("policies.yaml");
        if project_policies.exists() {
            return Some(project_policies);
        }
    }
    // 3. Global config
    if let Some(home) = dirs::home_dir() {
        let global_policies = home.join(".grid").join("policies.yaml");
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
        // ADR-V2-018: ContextDegraded was renamed to PostCompact. Both names
        // are accepted so existing YAML/JSON hook configs keep working; new
        // configs should prefer "PostCompact" or "PreCompact".
        "PostCompact" | "ContextDegraded" => Some(HookPoint::PostCompact),
        "PreCompact" => Some(HookPoint::PreCompact),
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
