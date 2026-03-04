use std::path::PathBuf;
use std::sync::Arc;

use octo_engine::{
    auth::AuthConfig,
    mcp::McpStorage,
    metrics::MetricsRegistry,
    scheduler::Scheduler,
    AgentExecutorHandle, AgentRuntime,
};

use crate::api::tasks::TaskStore;
use tokio::sync::RwLock;

use crate::config::Config;

pub struct AppState {
    pub db_path: PathBuf,
    /// Scheduler for periodic tasks (optional)
    pub scheduler: Option<Arc<Scheduler>>,
    /// Server configuration for frontend
    pub config: Config,
    /// Auth configuration for request authentication
    pub auth_config: AuthConfig,
    /// Metrics registry for collecting application metrics
    pub metrics_registry: Arc<RwLock<MetricsRegistry>>,
    /// Runtime supervisor: owns all agent dependencies and manages AgentExecutor lifecycle
    pub agent_supervisor: Arc<AgentRuntime>,
    /// 主 AgentExecutor 的通信句柄（channels 唯一的 Agent 接入点）
    pub agent_handle: AgentExecutorHandle,
    /// Background task store for one-off async tasks
    pub task_store: Option<Arc<TaskStore>>,
}

impl AppState {
    pub fn new(
        db_path: PathBuf,
        scheduler: Option<Arc<Scheduler>>,
        config: Config,
        agent_supervisor: Arc<AgentRuntime>,
        agent_handle: AgentExecutorHandle,
    ) -> Self {
        // Convert YAML config to runtime AuthConfig
        let auth_config = config.auth.to_auth_config();

        // Initialize metrics registry
        let metrics_registry = Arc::new(RwLock::new(MetricsRegistry::new()));

        // Initialize task store
        let task_store = Some(Arc::new(TaskStore::new()));

        Self {
            db_path,
            scheduler,
            config,
            auth_config,
            metrics_registry,
            agent_supervisor,
            agent_handle,
            task_store,
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
