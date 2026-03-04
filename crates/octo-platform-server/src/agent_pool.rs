//! Agent Pool Module
//!
//! Manages a pool of agent instances for multi-tenant platform.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use octo_engine::agent::{AgentCatalog, AgentRuntime, AgentRuntimeConfig};
use octo_engine::providers::ProviderConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;
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

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Pool errors
#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Pool exhausted: {current}/{max}")]
    Exhausted { current: u32, max: u32 },

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Instance not found: {0}")]
    NotFound(InstanceId),

    #[error("Instance busy: {0}")]
    Busy(InstanceId),

    #[error("Runtime error: {0}")]
    RuntimeError(String),
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

impl Workspace {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            session_ids: Vec::new(),
        }
    }
}

/// Agent instance
#[derive(Clone)]
pub struct AgentInstance {
    /// Instance ID
    pub id: InstanceId,
    /// Agent runtime (from octo-engine), wrapped in Arc for sharing
    pub runtime: Option<Arc<AgentRuntime>>,
    /// Current workspace (if occupied)
    pub workspace: Option<Workspace>,
    /// Current state
    pub state: InstanceState,
    /// Last used timestamp
    pub last_used: DateTime<Utc>,
}

impl fmt::Debug for AgentInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentInstance")
            .field("id", &self.id)
            .field("runtime", &"<AgentRuntime>")
            .field("workspace", &self.workspace)
            .field("state", &self.state)
            .field("last_used", &self.last_used)
            .finish()
    }
}

impl AgentInstance {
    /// Create a new agent instance (without runtime - used for placeholder/testing)
    #[allow(dead_code)]
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

    /// Get the total number of instances (alias for total_instances)
    pub fn total_count(&self) -> usize {
        self.instances.len()
    }

    /// Get the number of idle instances
    pub async fn idle_count(&self) -> usize {
        self.idle_instances.lock().await.len()
    }

    /// Get an instance from the pool
    pub async fn get_instance(&self, user_id: &str) -> Result<AgentInstance, PoolError> {
        // 1. Try to get from idle pool
        {
            let mut idle_instances = self.idle_instances.lock().await;
            if let Some(instance_id) = idle_instances.pop() {
                // Get instance and mark as busy
                if let Some(mut instance) = self.instances.get_mut(&instance_id) {
                    instance.state = InstanceState::Busy;
                    instance.workspace = Some(Workspace::new(user_id.to_string()));
                    instance.last_used = Utc::now();
                    return Ok(instance.clone());
                }
            }
        }

        // 2. No idle instances, check if we can create a new one
        let current_total = self.instances.len() as u32;
        if current_total >= self.config.hard_max_total {
            return Err(PoolError::Exhausted {
                current: current_total,
                max: self.config.hard_max_total,
            });
        }

        // 3. Create new instance (placeholder)
        let instance = self.create_instance(user_id).await?;
        Ok(instance)
    }

    /// Create a new agent instance with AgentRuntime from octo-engine
    async fn create_instance(&self, user_id: &str) -> Result<AgentInstance, PoolError> {
        // Create AgentRuntime configuration
        // Note: In production, these should come from pool configuration
        let provider_config = ProviderConfig {
            name: "anthropic".to_string(),
            api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            base_url: None,
            model: None,
        };

        let runtime_config = AgentRuntimeConfig::from_parts(
            // Use a temporary in-memory database path for this instance
            // In production, each instance could have its own DB or share one with isolation
            format!("/tmp/octo-platform-agent-{}.db", uuid::Uuid::new_v4()),
            provider_config,
            Vec::new(), // No skills dirs for now
            None,      // No provider chain
            Some(PathBuf::from("/tmp/octo-sandbox")),
            false, // Disable event bus for pool instances
        );

        // Create AgentCatalog (shared for all instances in the pool)
        let catalog = Arc::new(AgentCatalog::new());

        // Create the AgentRuntime
        let runtime = AgentRuntime::new(catalog, runtime_config)
            .await
            .map_err(|e| PoolError::RuntimeError(e.to_string()))?;

        let instance = AgentInstance {
            id: InstanceId::new(),
            runtime: Some(Arc::new(runtime)),
            workspace: Some(Workspace::new(user_id.to_string())),
            state: InstanceState::Busy,
            last_used: Utc::now(),
        };

        // Add to instances map
        self.instances.insert(instance.id.clone(), instance.clone());

        Ok(instance)
    }

    /// Release an instance back to the pool
    pub async fn release_instance(&self, instance_id: InstanceId) -> Result<(), PoolError> {
        // 1. Get the instance
        let mut instance = self
            .instances
            .get_mut(&instance_id)
            .ok_or(PoolError::NotFound(instance_id.clone()))?;

        // 2. State check
        if instance.state != InstanceState::Busy {
            return Err(PoolError::Busy(instance_id.clone()));
        }

        // 3. Clear workspace (isolation guarantee)
        instance.workspace = None;
        instance.state = InstanceState::Idle;
        instance.last_used = Utc::now();

        // 4. Add to idle pool
        let instance_id_clone = instance_id.clone();
        drop(instance);

        let mut idle_instances = self.idle_instances.lock().await;
        idle_instances.push(instance_id_clone);

        Ok(())
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
