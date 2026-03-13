use serde::{Deserialize, Serialize};

use crate::score::EvalScore;

/// Agent output collected from an evaluation run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentOutput {
    pub messages: Vec<octo_types::ChatMessage>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub rounds: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    pub stop_reason: String,
}

/// Record of a single tool call during evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub is_error: bool,
    pub duration_ms: u64,
}

/// Task metadata for categorization and filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub category: String,
    pub difficulty: Difficulty,
    pub expected_steps: Option<u32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// Configuration for LLM-based judge scoring
#[derive(Debug, Clone)]
pub struct LlmJudgeConfig {
    pub rubric: String,
    pub pass_threshold: f64,
}

/// Core evaluation task trait
pub trait EvalTask: Send + Sync {
    fn id(&self) -> &str;
    fn prompt(&self) -> &str;
    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>>;
    fn score(&self, output: &AgentOutput) -> EvalScore;
    fn metadata(&self) -> TaskMetadata;

    /// Optional tool name allowlist — when set, only these tools are available.
    /// The runner uses these names to filter the ToolRegistry.
    fn tool_allowlist(&self) -> Option<Vec<String>> {
        None
    }

    /// Optional LLM judge configuration for subjective scoring.
    /// When present, the runner will re-score using an LLM provider.
    fn llm_judge_config(&self) -> Option<LlmJudgeConfig> {
        None
    }
}
