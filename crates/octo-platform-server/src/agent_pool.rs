//! Agent Pool Module
//!
//! Manages a pool of agent instances for multi-tenant platform.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Step 1: Basic Data Structures
// ============================================================================

/// Agent instance state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceState {
    /// Idle, can be allocated
    Idle,
    /// Busy, working on a task
    Busy,
    /// Releasing, being returned to pool
    Releasing,
}

/// Isolation strategy for agent instances
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IsolationStrategy {
    /// Memory-level isolation (default)
    #[default]
    Memory,
    /// Process-level isolation
    Process,
    /// Session-level isolation
    Session,
}

/// Agent pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Soft limit, normal operating target
    pub soft_max_total: u32,
    /// Hard limit, cannot exceed
    pub hard_max_total: u32,
    /// Minimum idle instances (warm-up)
    pub min_idle: u32,
    /// Maximum idle instances (reclamation threshold)
    pub max_idle: u32,
    /// Idle timeout before reclamation
    pub idle_timeout: Duration,
    /// Isolation strategy
    pub strategy: IsolationStrategy,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            soft_max_total: 5,
            hard_max_total: 10,
            min_idle: 0,
            max_idle: 5,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            strategy: IsolationStrategy::Memory,
        }
    }
}

/// Agent instance ID
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceId(pub String);

impl InstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Step 2: Workspace and Agent Instance
// ============================================================================

/// User workspace - isolated memory/session context
#[derive(Debug, Clone)]
pub struct Workspace {
    /// User ID
    pub user_id: String,
    /// Session IDs in this workspace
    pub session_ids: Vec<String>,
}

/// Agent instance
#[derive(Debug)]
pub struct AgentInstance {
    /// Instance ID
    pub id: InstanceId,
    /// Agent runtime (from octo-engine)
    /// NOTE: Using Option<()> as placeholder - will integrate AgentRuntime in Task 3
    pub runtime: Option<()>,
    /// Current workspace (if occupied)
    pub workspace: Option<Workspace>,
    /// Current state
    pub state: InstanceState,
    /// Last used timestamp
    pub last_used: DateTime<Utc>,
}

impl AgentInstance {
    pub fn new() -> Self {
        Self {
            id: InstanceId::new(),
            runtime: None,
            workspace: None,
            state: InstanceState::Idle,
            last_used: Utc::now(),
        }
    }
}

impl Default for AgentInstance {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Step 3: Agent Pool Core Structure
// ============================================================================

/// Agent pool for managing agent instances
pub struct AgentPool {
    /// Pool configuration
    config: PoolConfig,
    /// All instances (active + idle)
    instances: DashMap<InstanceId, AgentInstance>,
    /// Idle instance IDs (for quick allocation)
    idle_instances: Arc<tokio::sync::Mutex<Vec<InstanceId>>>,
}

impl AgentPool {
    /// Create a new agent pool with default configuration
    pub fn new() -> Self {
        Self::with_config(PoolConfig::default())
    }

    /// Create a new agent pool with custom configuration
    pub fn with_config(config: PoolConfig) -> Self {
        Self {
            config,
            instances: DashMap::new(),
            idle_instances: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Get the pool configuration
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get the total number of instances
    pub fn total_instances(&self) -> usize {
        self.instances.len()
    }

    /// Get the number of idle instances
    pub async fn idle_count(&self) -> usize {
        self.idle_instances.lock().await.len()
    }
}

impl Default for AgentPool {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_id_generation() {
        let id1 = InstanceId::new();
        let id2 = InstanceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.soft_max_total, 5);
        assert_eq!(config.hard_max_total, 10);
        assert_eq!(config.min_idle, 0);
        assert_eq!(config.max_idle, 5);
        assert_eq!(config.idle_timeout, Duration::from_secs(300));
        assert_eq!(config.strategy, IsolationStrategy::Memory);
    }

    #[test]
    fn test_agent_instance_creation() {
        let instance = AgentInstance::new();
        assert_eq!(instance.state, InstanceState::Idle);
        assert!(instance.workspace.is_none());
    }

    #[test]
    fn test_agent_pool_creation() {
        let pool = AgentPool::new();
        assert_eq!(pool.total_instances(), 0);
    }
}
