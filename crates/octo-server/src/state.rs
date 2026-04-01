use std::path::PathBuf;
use std::sync::Arc;

use octo_engine::{
    auth::AuthConfig, mcp::McpStorage, metrics::MetricsRegistry, scheduler::Scheduler,
    tools::approval::ApprovalGate, AgentExecutorHandle, AgentRuntime,
};
use octo_types::SessionId;
use tokio::sync::RwLock;

use crate::config::Config;

/// Runtime-updatable configuration overrides (AO-T8).
/// Fields set to `Some(...)` override the corresponding value in `Config`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RuntimeConfigOverrides {
    pub logging_format: Option<String>,
    pub cors_strict: Option<bool>,
    pub cors_origins: Option<Vec<String>>,
    pub provider_name: Option<String>,
    pub provider_model: Option<String>,
    pub autonomy_level: Option<String>,
    pub require_approval_for_medium_risk: Option<bool>,
    pub block_high_risk_commands: Option<bool>,
}

pub struct AppState {
    pub db_path: PathBuf,
    /// Scheduler for periodic tasks (optional)
    pub scheduler: Option<Arc<Scheduler>>,
    /// Server configuration for frontend
    pub config: Config,
    /// Runtime-updatable overrides (AO-T8)
    pub runtime_overrides: RwLock<RuntimeConfigOverrides>,
    /// Auth configuration for request authentication
    pub auth_config: AuthConfig,
    /// Metrics registry for collecting application metrics
    pub metrics_registry: Arc<RwLock<MetricsRegistry>>,
    /// Runtime supervisor: owns all agent dependencies and manages AgentExecutor lifecycle
    pub agent_supervisor: Arc<AgentRuntime>,
    /// 主 AgentExecutor 的通信句柄（channels 唯一的 Agent 接入点）
    pub agent_handle: AgentExecutorHandle,
    /// Server start time for uptime calculation
    pub start_time: std::time::Instant,
    /// Shared approval gate for pending human approval requests (T3).
    pub approval_gate: Option<ApprovalGate>,
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

        // Create shared ApprovalGate — same instance shared between WS handler and AgentRuntime
        let approval_gate = agent_supervisor.approval_gate().cloned();

        Self {
            db_path,
            scheduler,
            config,
            runtime_overrides: RwLock::new(RuntimeConfigOverrides::default()),
            auth_config,
            metrics_registry,
            agent_supervisor,
            agent_handle,
            start_time: std::time::Instant::now(),
            approval_gate,
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

    /// Get metering storage on-demand (creates new async DB connection each time)
    pub async fn metering_storage(&self) -> Option<octo_engine::metering::storage::MeteringStorage> {
        let db = octo_engine::Database::open(self.db_path.to_str()?).await.ok()?;
        Some(octo_engine::metering::storage::MeteringStorage::new(db))
    }

    /// Resolve a session handle: if session_id is given, look up in agent_supervisor;
    /// otherwise return the primary agent_handle.
    #[allow(dead_code)]
    pub fn resolve_session_handle(&self, session_id: Option<&str>) -> Option<AgentExecutorHandle> {
        match session_id {
            Some(id) => {
                let sid = SessionId::from_string(id);
                self.agent_supervisor.get_session_handle(&sid)
            }
            None => Some(self.agent_handle.clone()),
        }
    }
}
