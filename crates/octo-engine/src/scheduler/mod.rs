//! Scheduler module for periodic task execution

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Scheduled task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub user_id: Option<String>,
    pub name: String,
    pub cron: String,
    pub agent_config: AgentTaskConfig,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Agent task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskConfig {
    pub system_prompt: String,
    pub input: String,
    pub max_rounds: u32,
    pub timeout_secs: u64,
}

impl Default for AgentTaskConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            input: String::new(),
            max_rounds: 50,
            timeout_secs: 300,
        }
    }
}

/// Task execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: ExecutionStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
    Timeout,
    Cancelled,
}

/// Scheduler errors
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("Task already running: {0}")]
    TaskAlreadyRunning(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

impl Serialize for SchedulerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
