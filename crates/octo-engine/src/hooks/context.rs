use serde_json::Value;
use std::collections::HashMap;

/// Context passed to hook handlers
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    /// Session ID (if applicable)
    pub session_id: Option<String>,
    /// Tool name (for tool hooks)
    pub tool_name: Option<String>,
    /// Tool input (for PreToolUse)
    pub tool_input: Option<Value>,
    /// Tool result (for PostToolUse)
    pub tool_result: Option<Value>,
    /// Task/prompt text
    pub task: Option<String>,
    /// Agent ID
    pub agent_id: Option<String>,
    /// Turn number
    pub turn: Option<u32>,
    /// Duration in ms (for post-hooks)
    pub duration_ms: Option<u64>,
    /// Whether the operation succeeded (for post-hooks)
    pub success: Option<bool>,
    /// Arbitrary metadata
    pub metadata: HashMap<String, Value>,
    /// Degradation level (for ContextDegraded hook)
    pub degradation_level: Option<String>,
    /// Redirect target agent or tool (for Redirect action)
    pub redirect_target: Option<String>,
}

impl HookContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_tool(mut self, name: impl Into<String>, input: Value) -> Self {
        self.tool_name = Some(name.into());
        self.tool_input = Some(input);
        self
    }

    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_turn(mut self, turn: u32) -> Self {
        self.turn = Some(turn);
        self
    }

    pub fn with_result(mut self, success: bool, duration_ms: u64) -> Self {
        self.success = Some(success);
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_degradation(mut self, level: impl Into<String>) -> Self {
        self.degradation_level = Some(level.into());
        self
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }
}
