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
    /// Skill name (for skill hooks)
    pub skill_name: Option<String>,
    /// Activated skills list (for SkillsActivated)
    pub activated_skills: Option<Vec<String>>,
    /// Query that triggered skill activation
    pub activation_query: Option<String>,
    /// Script being executed (for SkillScriptStarted)
    pub script_path: Option<String>,
    /// Runtime type (for SkillScriptStarted)
    pub runtime_type: Option<String>,
    /// Constraint violation reason (for ToolConstraintViolated)
    pub constraint_reason: Option<String>,
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

    pub fn with_skill(mut self, name: impl Into<String>) -> Self {
        self.skill_name = Some(name.into());
        self
    }

    pub fn with_activated_skills(mut self, skills: Vec<String>, query: impl Into<String>) -> Self {
        self.activated_skills = Some(skills);
        self.activation_query = Some(query.into());
        self
    }

    pub fn with_script(mut self, path: impl Into<String>, runtime: impl Into<String>) -> Self {
        self.script_path = Some(path.into());
        self.runtime_type = Some(runtime.into());
        self
    }

    pub fn with_constraint_violation(
        mut self,
        tool: impl Into<String>,
        skill: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        self.tool_name = Some(tool.into());
        self.skill_name = Some(skill.into());
        self.constraint_reason = Some(reason.into());
        self
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }
}
