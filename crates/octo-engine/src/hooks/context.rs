use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Context passed to hook handlers.
///
/// Contains event info, tool details, runtime environment, and history
/// for all three hook layers (programmatic, policy engine, declarative).
#[derive(Debug, Clone, Default, Serialize)]
pub struct HookContext {
    // === Event / session ===
    /// Session ID (if applicable)
    pub session_id: Option<String>,
    /// Agent ID
    pub agent_id: Option<String>,
    /// Turn number
    pub turn: Option<u32>,

    // === Tool info ===
    /// Tool name (for tool hooks)
    pub tool_name: Option<String>,
    /// Tool input (for PreToolUse)
    pub tool_input: Option<Value>,
    /// Tool result (for PostToolUse)
    pub tool_result: Option<Value>,
    /// Duration in ms (for post-hooks)
    pub duration_ms: Option<u64>,
    /// Whether the operation succeeded (for post-hooks)
    pub success: Option<bool>,

    // === Task ===
    /// Task/prompt text
    pub task: Option<String>,

    // === Runtime environment (Phase AH) ===
    /// Working directory for the current session.
    pub working_dir: Option<String>,
    /// Sandbox mode: "host", "docker", "wasm".
    pub sandbox_mode: Option<String>,
    /// Sandbox profile: "development", "staging", "production", "custom".
    pub sandbox_profile: Option<String>,
    /// LLM model name.
    pub model: Option<String>,
    /// Autonomy level: "full", "supervised", "restricted".
    pub autonomy_level: Option<String>,

    // === History (Phase AH) ===
    /// Total tool calls so far in this session turn.
    pub total_tool_calls: Option<u32>,
    /// Current round number.
    pub current_round: Option<u32>,
    /// Recent tool names (last N calls).
    pub recent_tools: Option<Vec<String>>,

    // === User input (Phase AH) ===
    /// The user's original query for this turn.
    pub user_query: Option<String>,

    // === Metadata ===
    /// Arbitrary metadata
    pub metadata: HashMap<String, Value>,

    // === Degradation ===
    /// Degradation level (for ContextDegraded hook)
    pub degradation_level: Option<String>,

    // === Routing ===
    /// Redirect target agent or tool (for Redirect action)
    pub redirect_target: Option<String>,

    // === Skill info ===
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

    // === Phase AH: Environment context builder ===

    /// Set runtime environment fields.
    pub fn with_environment(
        mut self,
        working_dir: impl Into<String>,
        sandbox_mode: impl Into<String>,
        sandbox_profile: impl Into<String>,
        model: impl Into<String>,
        autonomy_level: impl Into<String>,
    ) -> Self {
        self.working_dir = Some(working_dir.into());
        self.sandbox_mode = Some(sandbox_mode.into());
        self.sandbox_profile = Some(sandbox_profile.into());
        self.model = Some(model.into());
        self.autonomy_level = Some(autonomy_level.into());
        self
    }

    /// Set history fields (tool call counts and recent tool names).
    pub fn with_history(
        mut self,
        total_calls: u32,
        round: u32,
        recent: Vec<String>,
    ) -> Self {
        self.total_tool_calls = Some(total_calls);
        self.current_round = Some(round);
        self.recent_tools = Some(recent);
        self
    }

    /// Set the user's original query for this turn.
    pub fn with_user_query(mut self, query: impl Into<String>) -> Self {
        self.user_query = Some(query.into());
        self
    }

    // === Phase AH: Serialization helpers ===

    /// Serialize the context to a JSON Value for external hooks.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Serialize to a flat list of environment variables with OCTO_ prefix.
    ///
    /// Used by command-type declarative hooks to pass context via env vars.
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        if let Some(ref s) = self.session_id {
            vars.push(("OCTO_SESSION_ID".into(), s.clone()));
        }
        if let Some(ref a) = self.agent_id {
            vars.push(("OCTO_AGENT_ID".into(), a.clone()));
        }
        if let Some(turn) = self.turn {
            vars.push(("OCTO_TURN".into(), turn.to_string()));
        }
        if let Some(ref t) = self.tool_name {
            vars.push(("OCTO_TOOL_NAME".into(), t.clone()));
        }
        if let Some(ref w) = self.working_dir {
            vars.push(("OCTO_WORKING_DIR".into(), w.clone()));
        }
        if let Some(ref m) = self.sandbox_mode {
            vars.push(("OCTO_SANDBOX_MODE".into(), m.clone()));
        }
        if let Some(ref p) = self.sandbox_profile {
            vars.push(("OCTO_SANDBOX_PROFILE".into(), p.clone()));
        }
        if let Some(ref m) = self.model {
            vars.push(("OCTO_MODEL".into(), m.clone()));
        }
        if let Some(ref a) = self.autonomy_level {
            vars.push(("OCTO_AUTONOMY_LEVEL".into(), a.clone()));
        }
        if let Some(total) = self.total_tool_calls {
            vars.push(("OCTO_TOTAL_TOOL_CALLS".into(), total.to_string()));
        }
        if let Some(round) = self.current_round {
            vars.push(("OCTO_CURRENT_ROUND".into(), round.to_string()));
        }
        vars
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context_environment() {
        let ctx = HookContext::new()
            .with_session("s1")
            .with_environment("/tmp/proj", "host", "development", "claude-sonnet", "supervised");
        assert_eq!(ctx.working_dir.as_deref(), Some("/tmp/proj"));
        assert_eq!(ctx.sandbox_mode.as_deref(), Some("host"));
        assert_eq!(ctx.sandbox_profile.as_deref(), Some("development"));
        assert_eq!(ctx.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(ctx.autonomy_level.as_deref(), Some("supervised"));
    }

    #[test]
    fn test_hook_context_history() {
        let ctx = HookContext::new()
            .with_history(12, 3, vec!["bash".into(), "file_read".into()]);
        assert_eq!(ctx.total_tool_calls, Some(12));
        assert_eq!(ctx.current_round, Some(3));
        assert_eq!(ctx.recent_tools.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_hook_context_user_query() {
        let ctx = HookContext::new().with_user_query("Show me the code");
        assert_eq!(ctx.user_query.as_deref(), Some("Show me the code"));
    }

    #[test]
    fn test_hook_context_to_json() {
        let ctx = HookContext::new()
            .with_session("s1")
            .with_tool("bash", serde_json::json!({"command": "ls"}));
        let json = ctx.to_json();
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["tool_name"], "bash");
        assert_eq!(json["tool_input"]["command"], "ls");
    }

    #[test]
    fn test_hook_context_to_json_with_environment() {
        let ctx = HookContext::new()
            .with_environment("/tmp", "docker", "production", "gpt-4o", "restricted")
            .with_history(5, 2, vec!["file_read".into()]);
        let json = ctx.to_json();
        assert_eq!(json["working_dir"], "/tmp");
        assert_eq!(json["sandbox_mode"], "docker");
        assert_eq!(json["sandbox_profile"], "production");
        assert_eq!(json["total_tool_calls"], 5);
        assert_eq!(json["current_round"], 2);
    }

    #[test]
    fn test_hook_context_to_env_vars() {
        let ctx = HookContext::new()
            .with_session("s1")
            .with_environment("/tmp", "host", "dev", "model-x", "full")
            .with_history(10, 3, vec![]);
        let vars = ctx.to_env_vars();
        assert!(vars.iter().any(|(k, v)| k == "OCTO_SESSION_ID" && v == "s1"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_SANDBOX_MODE" && v == "host"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_WORKING_DIR" && v == "/tmp"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_MODEL" && v == "model-x"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_AUTONOMY_LEVEL" && v == "full"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_TOTAL_TOOL_CALLS" && v == "10"));
        assert!(vars.iter().any(|(k, v)| k == "OCTO_CURRENT_ROUND" && v == "3"));
    }

    #[test]
    fn test_hook_context_to_env_vars_minimal() {
        let ctx = HookContext::new();
        let vars = ctx.to_env_vars();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_hook_context_default_is_empty() {
        let ctx = HookContext::default();
        assert!(ctx.session_id.is_none());
        assert!(ctx.working_dir.is_none());
        assert!(ctx.sandbox_mode.is_none());
        assert!(ctx.total_tool_calls.is_none());
        assert!(ctx.recent_tools.is_none());
        assert!(ctx.user_query.is_none());
    }

    #[test]
    fn test_hook_context_chained_builders() {
        let ctx = HookContext::new()
            .with_session("sess-1")
            .with_agent("agent-1")
            .with_turn(5)
            .with_tool("bash", serde_json::json!({"command": "ls"}))
            .with_environment("/work", "host", "staging", "sonnet", "supervised")
            .with_history(20, 5, vec!["bash".into(), "file_read".into(), "file_write".into()])
            .with_user_query("List files");

        assert_eq!(ctx.session_id.as_deref(), Some("sess-1"));
        assert_eq!(ctx.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(ctx.turn, Some(5));
        assert_eq!(ctx.tool_name.as_deref(), Some("bash"));
        assert_eq!(ctx.working_dir.as_deref(), Some("/work"));
        assert_eq!(ctx.total_tool_calls, Some(20));
        assert_eq!(ctx.recent_tools.as_ref().unwrap().len(), 3);
        assert_eq!(ctx.user_query.as_deref(), Some("List files"));

        // Verify JSON roundtrip includes all fields
        let json = ctx.to_json();
        assert_eq!(json["session_id"], "sess-1");
        assert_eq!(json["working_dir"], "/work");
        assert_eq!(json["user_query"], "List files");
    }
}
