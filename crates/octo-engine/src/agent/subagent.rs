use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use octo_types::ChatMessage;

/// Handle to a running sub-agent
#[derive(Debug, Clone)]
pub struct SubAgentHandle {
    pub id: String,
    pub description: String,
    pub status: SubAgentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubAgentStatus {
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Task definition for a sub-agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentTask {
    /// Description of what the sub-agent should do
    pub description: String,
    /// Initial context messages for the sub-agent
    pub context: Vec<ChatMessage>,
    /// Tool whitelist (None = all tools available)
    pub tools: Option<Vec<String>>,
    /// Maximum iterations for the sub-agent loop
    pub max_iterations: u32,
}

/// Result from a completed sub-agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub id: String,
    pub status: SubAgentStatus,
    pub output: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub iterations_used: u32,
}

/// Manages sub-agent spawning with recursion depth limits
#[derive(Debug)]
pub struct SubAgentManager {
    active_agents: Arc<Mutex<HashMap<String, SubAgentHandle>>>,
    max_concurrent: usize,
    max_depth: usize,
    current_depth: usize,
}

impl SubAgentManager {
    pub fn new(max_concurrent: usize, max_depth: usize) -> Self {
        Self {
            active_agents: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent,
            max_depth,
            current_depth: 0,
        }
    }

    /// Create a child manager with incremented depth
    pub fn child(&self) -> Result<Self> {
        let next_depth = self.current_depth + 1;
        if next_depth >= self.max_depth {
            bail!(
                "SubAgent recursion depth limit reached: {}/{}",
                next_depth,
                self.max_depth
            );
        }
        Ok(Self {
            active_agents: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent: self.max_concurrent,
            max_depth: self.max_depth,
            current_depth: next_depth,
        })
    }

    /// Check if we can spawn more sub-agents
    pub async fn can_spawn(&self) -> bool {
        let agents = self.active_agents.lock().await;
        agents
            .values()
            .filter(|h| h.status == SubAgentStatus::Running)
            .count()
            < self.max_concurrent
    }

    /// Register a new sub-agent
    pub async fn register(&self, id: String, description: String) -> Result<()> {
        if !self.can_spawn().await {
            bail!(
                "Maximum concurrent sub-agents reached: {}",
                self.max_concurrent
            );
        }
        let handle = SubAgentHandle {
            id: id.clone(),
            description,
            status: SubAgentStatus::Running,
        };
        let mut agents = self.active_agents.lock().await;
        agents.insert(id, handle);
        Ok(())
    }

    /// Mark a sub-agent as completed
    pub async fn complete(&self, id: &str, output: Option<String>) -> Result<SubAgentResult> {
        let mut agents = self.active_agents.lock().await;
        if let Some(handle) = agents.get_mut(id) {
            handle.status = SubAgentStatus::Completed;
            Ok(SubAgentResult {
                id: id.to_string(),
                status: SubAgentStatus::Completed,
                output,
                messages: Vec::new(),
                iterations_used: 0,
            })
        } else {
            bail!("SubAgent not found: {}", id)
        }
    }

    /// Mark a sub-agent as failed
    pub async fn fail(&self, id: &str, error: String) -> Result<()> {
        let mut agents = self.active_agents.lock().await;
        if let Some(handle) = agents.get_mut(id) {
            handle.status = SubAgentStatus::Failed(error);
            Ok(())
        } else {
            bail!("SubAgent not found: {}", id)
        }
    }

    /// Cancel a running sub-agent
    pub async fn cancel(&self, id: &str) -> Result<()> {
        let mut agents = self.active_agents.lock().await;
        if let Some(handle) = agents.get_mut(id) {
            handle.status = SubAgentStatus::Cancelled;
            Ok(())
        } else {
            bail!("SubAgent not found: {}", id)
        }
    }

    /// List all sub-agents
    pub async fn list(&self) -> Vec<SubAgentHandle> {
        let agents = self.active_agents.lock().await;
        agents.values().cloned().collect()
    }

    /// Get current recursion depth
    pub fn depth(&self) -> usize {
        self.current_depth
    }

    /// Get max allowed depth
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Cancel all running sub-agents
    pub async fn cancel_all(&self) {
        let mut agents = self.active_agents.lock().await;
        for handle in agents.values_mut() {
            if handle.status == SubAgentStatus::Running {
                handle.status = SubAgentStatus::Cancelled;
            }
        }
    }
}
