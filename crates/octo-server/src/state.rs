use std::path::PathBuf;
use std::sync::Arc;

use octo_engine::{
    auth::AuthConfig,
    mcp::{McpManager, McpStorage},
    metrics::MetricsRegistry,
    providers::ProviderChain,
    scheduler::Scheduler,
    AgentCatalog, AgentRuntimeHandle, AgentSupervisor, MemoryStore, SessionStore, SkillRegistry,
    ToolExecutionRecorder, ToolRegistry, WorkingMemory,
};
use tokio::sync::RwLock;

use crate::config::Config;

pub struct AppState {
    /// Provider chain for LLM failover (optional), stored as Arc for cheap cloning
    pub provider_chain: Option<Arc<ProviderChain>>,
    pub tools: Arc<ToolRegistry>,
    pub memory: Arc<dyn WorkingMemory>,
    pub sessions: Arc<dyn SessionStore>,
    pub memory_store: Arc<dyn MemoryStore>,
    pub db_path: PathBuf,
    pub mcp_manager: Arc<tokio::sync::Mutex<McpManager>>,
    pub model: Option<String>,
    pub recorder: Option<Arc<ToolExecutionRecorder>>,
    #[allow(dead_code)]
    pub skill_registry: Arc<SkillRegistry>,
    /// Scheduler for periodic tasks (optional)
    pub scheduler: Option<Arc<Scheduler>>,
    /// Server configuration for frontend
    pub config: Config,
    /// Auth configuration for request authentication
    pub auth_config: AuthConfig,
    /// Metrics registry for collecting application metrics
    pub metrics_registry: Arc<RwLock<MetricsRegistry>>,
    /// Agent catalog for agent definitions and lifecycle state
    pub catalog: Arc<AgentCatalog>,
    /// Runtime supervisor: holds shared deps and manages AgentRuntime lifecycle
    pub agent_supervisor: Arc<AgentSupervisor>,
    /// 主 AgentRuntime 的通信句柄（channels 唯一的 Agent 接入点）。
    /// channels 通过此 handle 发消息、订阅事件，无需持有 AgentSupervisor。
    pub agent_handle: AgentRuntimeHandle,
}

impl AppState {
    pub fn new(
        provider_chain: Option<Arc<ProviderChain>>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        sessions: Arc<dyn SessionStore>,
        memory_store: Arc<dyn MemoryStore>,
        db_path: PathBuf,
        mcp_manager: Arc<tokio::sync::Mutex<McpManager>>,
        model: Option<String>,
        recorder: Option<Arc<ToolExecutionRecorder>>,
        skill_registry: Arc<SkillRegistry>,
        scheduler: Option<Arc<Scheduler>>,
        config: Config,
        catalog: Arc<AgentCatalog>,
        agent_supervisor: Arc<AgentSupervisor>,
        agent_handle: AgentRuntimeHandle,
    ) -> Self {
        // Convert YAML config to runtime AuthConfig
        let auth_config = config.auth.to_auth_config();

        // Initialize metrics registry
        let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new()));

        Self {
            provider_chain,
            tools,
            memory,
            sessions,
            memory_store,
            db_path,
            mcp_manager,
            model,
            recorder,
            skill_registry,
            scheduler,
            config,
            auth_config,
            metrics_registry,
            catalog,
            agent_supervisor,
            agent_handle,
        }
    }

    /// Get MCP storage on-demand (creates new connection each time)
    pub fn mcp_storage(&self) -> Option<octo_engine::mcp::storage::McpStorage> {
        McpStorage::new(&self.db_path).ok()
    }

    /// Get audit storage on-demand (creates new connection each time)
    pub fn audit_storage(&self) -> Option<octo_engine::audit::AuditStorage> {
        octo_engine::audit::AuditStorage::new(&self.db_path).ok()
    }
}
