use serde::{Deserialize, Serialize};

use crate::ToolSource;

/// Status of a tool execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
    Timeout,
}

/// Record of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub tool_name: String,
    pub source: ToolSource,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub status: ExecutionStatus,
    pub started_at: i64,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Snapshot of the token budget state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudgetSnapshot {
    pub total: usize,
    pub system_prompt: usize,
    pub dynamic_context: usize,
    pub history: usize,
    pub free: usize,
    pub usage_percent: f32,
    pub degradation_level: u8,
}
