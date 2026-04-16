use serde::{Serialize, Serializer};
use serde_json::Value;
use std::collections::HashMap;

/// Context passed to hook handlers.
///
/// Contains event info, tool details, runtime environment, and history
/// for all three hook layers (programmatic, policy engine, declarative).
///
/// ## ADR-V2-006 §2/§3 envelope parity (D120)
///
/// Phase 2.5 S0.T3 added five fields that make the Rust envelope byte-
/// equivalent with the Python runtime envelope (see
/// `lang/claude-code-runtime-python/src/claude_code_runtime/service.py`
/// around `_dispatch_scoped_pre_tool_use` / `_dispatch_scoped_post_tool_use`
/// / `_dispatch_scoped_stop`):
///
/// - `event`               — `"PreToolUse"` / `"PostToolUse"` / `"Stop"`.
/// - `skill_id`            — empty string when no skill attached.
/// - `draft_memory_id`     — Stop envelopes only, empty string if absent.
/// - `evidence_anchor_id`  — Stop envelopes only, empty string if absent.
/// - `created_at`          — ISO-8601 Zulu, auto-set in `new()`.
///
/// [`to_json`] switches on `event` to emit the canonical shape for
/// that scope; legacy callers (no `event` set) still receive the
/// full struct projection for backwards compatibility with Phase 2
/// declarative hook consumers.
#[derive(Debug, Clone, Serialize)]
pub struct HookContext {
    // === ADR-V2-006 §2 envelope (required across all scopes) ===
    /// Hook event type: "PreToolUse" | "PostToolUse" | "Stop".
    /// When `None`, `to_json()` falls back to the legacy full-struct shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Skill identifier. Per ADR §2 MUST be empty string "", not null, when no
    /// skill is attached. Represented as `Option<String>` internally so that
    /// legacy code paths that never set it stay a clean `None`; the canonical
    /// envelope emitter in `to_json()` maps `None -> ""`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_opt_as_empty_str"
    )]
    pub skill_id: Option<String>,
    /// Stop envelope only. `None` and `Some("")` both surface as `""` per
    /// ADR §2.3.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_opt_as_empty_str"
    )]
    pub draft_memory_id: Option<String>,
    /// Stop envelope only. Same empty-string semantics as `draft_memory_id`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_opt_as_empty_str"
    )]
    pub evidence_anchor_id: Option<String>,
    /// ISO-8601 UTC "Z" timestamp, second precision (e.g.
    /// `"2026-04-16T14:30:00Z"`). Auto-populated by `new()` via
    /// `chrono::Utc::now()`; overridable via `with_created_at()` for
    /// deterministic tests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

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

/// Serde helper: emit `Option<String>` as empty-string when `None`.
///
/// Used for ADR-V2-006 §2.3 optional fields that MUST NOT serialize as
/// JSON `null`. Paired with `#[serde(skip_serializing_if)]` we still get
/// "skip when not relevant" for the legacy full-struct projection, while
/// the canonical envelope emitter in `to_json()` forces the value through
/// this helper so an explicit `Some("")` or a present-but-empty field
/// round-trips as `""`.
fn serialize_opt_as_empty_str<S>(opt: &Option<String>, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_str(opt.as_deref().unwrap_or(""))
}

impl Default for HookContext {
    fn default() -> Self {
        Self {
            event: None,
            skill_id: None,
            draft_memory_id: None,
            evidence_anchor_id: None,
            created_at: None,
            session_id: None,
            agent_id: None,
            turn: None,
            tool_name: None,
            tool_input: None,
            tool_result: None,
            duration_ms: None,
            success: None,
            task: None,
            working_dir: None,
            sandbox_mode: None,
            sandbox_profile: None,
            model: None,
            autonomy_level: None,
            total_tool_calls: None,
            current_round: None,
            recent_tools: None,
            user_query: None,
            metadata: HashMap::new(),
            degradation_level: None,
            redirect_target: None,
            skill_name: None,
            activated_skills: None,
            activation_query: None,
            script_path: None,
            runtime_type: None,
            constraint_reason: None,
        }
    }
}

impl HookContext {
    /// Construct an empty `HookContext` with `created_at` auto-populated
    /// to `chrono::Utc::now()` in ADR-V2-006 §2.4 format
    /// (`"%Y-%m-%dT%H:%M:%SZ"`).
    ///
    /// Tests that need deterministic timestamps should override via
    /// [`with_created_at`](Self::with_created_at) immediately after
    /// construction.
    pub fn new() -> Self {
        Self {
            created_at: Some(current_iso8601_z()),
            ..Self::default()
        }
    }

    // === ADR-V2-006 §2/§3 envelope builders ===

    /// Set the envelope `event` — one of `"PreToolUse"`, `"PostToolUse"`,
    /// `"Stop"`. This switches [`to_json`](Self::to_json) from the legacy
    /// full-struct projection to the canonical ADR-V2-006 envelope.
    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Set the skill id. Empty string is the correct value when no skill
    /// is attached — prefer `with_skill_id("")` over leaving `None` for
    /// hook paths that need envelope compliance.
    pub fn with_skill_id(mut self, skill_id: impl Into<String>) -> Self {
        self.skill_id = Some(skill_id.into());
        self
    }

    /// Set the Stop-scope `draft_memory_id`. `None` and `Some("")` are
    /// semantically identical per ADR §2.3 — both serialize to `""`.
    pub fn with_draft_memory_id(mut self, id: impl Into<String>) -> Self {
        self.draft_memory_id = Some(id.into());
        self
    }

    /// Set the Stop-scope `evidence_anchor_id`. See `with_draft_memory_id`
    /// for empty-string semantics.
    pub fn with_evidence_anchor_id(mut self, id: impl Into<String>) -> Self {
        self.evidence_anchor_id = Some(id.into());
        self
    }

    /// Override the auto-populated `created_at` with an explicit timestamp.
    /// Callers are responsible for supplying an ISO-8601 UTC "Z" string.
    pub fn with_created_at(mut self, ts: impl Into<String>) -> Self {
        self.created_at = Some(ts.into());
        self
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
    pub fn with_history(mut self, total_calls: u32, round: u32, recent: Vec<String>) -> Self {
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

    // === Phase AH + ADR-V2-006 §2: Serialization helpers ===

    /// Serialize the context to a JSON Value for external hooks.
    ///
    /// When `event` is set (ADR-V2-006 path), produces the canonical
    /// PreToolUse / PostToolUse / Stop envelope with exactly the keys
    /// from §2.1/§2.2/§2.3 — byte-parity target with the Python runtime.
    /// When `event` is `None`, falls back to the legacy full-struct
    /// projection for pre-ADR declarative-hook consumers.
    pub fn to_json(&self) -> serde_json::Value {
        match self.event.as_deref() {
            Some("PreToolUse") => self.to_pre_tool_use_envelope(),
            Some("PostToolUse") => self.to_post_tool_use_envelope(),
            Some("Stop") => self.to_stop_envelope(),
            // Unknown event or None: emit full struct for backwards
            // compatibility. Hooks written pre-ADR continue to see
            // every field they were built against.
            _ => serde_json::to_value(self).unwrap_or_default(),
        }
    }

    /// Emit the ADR-V2-006 §2.1 PreToolUse envelope.
    fn to_pre_tool_use_envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "PreToolUse",
            "session_id": self.session_id.clone().unwrap_or_default(),
            "skill_id": self.skill_id.clone().unwrap_or_default(),
            "tool_name": self.tool_name.clone().unwrap_or_default(),
            "tool_args": self
                .tool_input
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
            "created_at": self
                .created_at
                .clone()
                .unwrap_or_else(current_iso8601_z),
        })
    }

    /// Emit the ADR-V2-006 §2.2 PostToolUse envelope.
    fn to_post_tool_use_envelope(&self) -> serde_json::Value {
        // tool_result in the envelope is a string (serialized output) per
        // §2.2. If the caller populated `tool_result` with a JSON value
        // we serialize it to a compact string; if they used `success`
        // (a boolean) we fall back to an empty string. `is_error` is
        // the inverse of `success`; when absent we treat as not-error.
        let tool_result_str = match &self.tool_result {
            Some(Value::String(s)) => s.clone(),
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => String::new(),
        };
        let is_error = match self.success {
            Some(ok) => !ok,
            None => false,
        };
        serde_json::json!({
            "event": "PostToolUse",
            "session_id": self.session_id.clone().unwrap_or_default(),
            "skill_id": self.skill_id.clone().unwrap_or_default(),
            "tool_name": self.tool_name.clone().unwrap_or_default(),
            "tool_result": tool_result_str,
            "is_error": is_error,
            "created_at": self
                .created_at
                .clone()
                .unwrap_or_else(current_iso8601_z),
        })
    }

    /// Emit the ADR-V2-006 §2.3 Stop envelope.
    fn to_stop_envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "Stop",
            "session_id": self.session_id.clone().unwrap_or_default(),
            "skill_id": self.skill_id.clone().unwrap_or_default(),
            "draft_memory_id": self.draft_memory_id.clone().unwrap_or_default(),
            "evidence_anchor_id": self.evidence_anchor_id.clone().unwrap_or_default(),
            "created_at": self
                .created_at
                .clone()
                .unwrap_or_else(current_iso8601_z),
        })
    }

    /// Serialize to a flat list of environment variables with `GRID_` prefix.
    ///
    /// Used by command-type declarative hooks to pass context via env vars.
    ///
    /// ADR-V2-006 §3 MUSTs (always emitted when `event` is set):
    ///   `GRID_SESSION_ID`, `GRID_TOOL_NAME` (may be empty for Stop),
    ///   `GRID_SKILL_ID` (may be empty), `GRID_EVENT`.
    ///
    /// Pre-ADR env vars (`GRID_AGENT_ID`, `GRID_TURN`, `GRID_WORKING_DIR`,
    /// …) are still emitted when their source fields are populated — they
    /// are not forbidden by §3, just not required.
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        // ADR §3 MUST: always emit GRID_SESSION_ID/TOOL_NAME/SKILL_ID/EVENT
        // whenever we are producing an ADR envelope. For legacy callers
        // without `event` we keep the old "only emit what's set" behavior
        // so untouched consumers remain byte-identical.
        let in_envelope_mode = self.event.is_some();

        if in_envelope_mode {
            vars.push((
                "GRID_SESSION_ID".into(),
                self.session_id.clone().unwrap_or_default(),
            ));
            vars.push((
                "GRID_TOOL_NAME".into(),
                self.tool_name.clone().unwrap_or_default(),
            ));
            vars.push((
                "GRID_SKILL_ID".into(),
                self.skill_id.clone().unwrap_or_default(),
            ));
            vars.push(("GRID_EVENT".into(), self.event.clone().unwrap_or_default()));
        } else {
            if let Some(ref s) = self.session_id {
                vars.push(("GRID_SESSION_ID".into(), s.clone()));
            }
            if let Some(ref t) = self.tool_name {
                vars.push(("GRID_TOOL_NAME".into(), t.clone()));
            }
        }

        // Optional / Phase AH env vars — non-conflicting with ADR §3
        // because §3 only specifies the MUST-SET minimum; it does not
        // forbid additional GRID_* variables added by Phase AH predating
        // the ADR. When a future ADR tightens §3 these can be revisited.
        if let Some(ref a) = self.agent_id {
            vars.push(("GRID_AGENT_ID".into(), a.clone()));
        }
        if let Some(turn) = self.turn {
            vars.push(("GRID_TURN".into(), turn.to_string()));
        }
        if let Some(ref w) = self.working_dir {
            vars.push(("GRID_WORKING_DIR".into(), w.clone()));
        }
        if let Some(ref m) = self.sandbox_mode {
            vars.push(("GRID_SANDBOX_MODE".into(), m.clone()));
        }
        if let Some(ref p) = self.sandbox_profile {
            vars.push(("GRID_SANDBOX_PROFILE".into(), p.clone()));
        }
        if let Some(ref m) = self.model {
            vars.push(("GRID_MODEL".into(), m.clone()));
        }
        if let Some(ref a) = self.autonomy_level {
            vars.push(("GRID_AUTONOMY_LEVEL".into(), a.clone()));
        }
        if let Some(total) = self.total_tool_calls {
            vars.push(("GRID_TOTAL_TOOL_CALLS".into(), total.to_string()));
        }
        if let Some(round) = self.current_round {
            vars.push(("GRID_CURRENT_ROUND".into(), round.to_string()));
        }
        vars
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }
}

/// Produce an ADR-V2-006 §2.4 compliant timestamp — ISO-8601 UTC with
/// `Z` suffix, second precision (`"%Y-%m-%dT%H:%M:%SZ"`). Mirrors the
/// Python runtime's
/// `datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")`.
fn current_iso8601_z() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context_environment() {
        let ctx = HookContext::new().with_session("s1").with_environment(
            "/tmp/proj",
            "host",
            "development",
            "claude-sonnet",
            "supervised",
        );
        assert_eq!(ctx.working_dir.as_deref(), Some("/tmp/proj"));
        assert_eq!(ctx.sandbox_mode.as_deref(), Some("host"));
        assert_eq!(ctx.sandbox_profile.as_deref(), Some("development"));
        assert_eq!(ctx.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(ctx.autonomy_level.as_deref(), Some("supervised"));
    }

    #[test]
    fn test_hook_context_history() {
        let ctx = HookContext::new().with_history(12, 3, vec!["bash".into(), "file_read".into()]);
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
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_SESSION_ID" && v == "s1"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_SANDBOX_MODE" && v == "host"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_WORKING_DIR" && v == "/tmp"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_MODEL" && v == "model-x"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_AUTONOMY_LEVEL" && v == "full"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_TOTAL_TOOL_CALLS" && v == "10"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "GRID_CURRENT_ROUND" && v == "3"));
    }

    #[test]
    fn test_hook_context_to_env_vars_minimal() {
        // Explicitly use Default to avoid new()'s automatic created_at.
        let ctx = HookContext::default();
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
        // ADR-V2-006 fields default to None on a raw Default() instance;
        // new() populates created_at separately.
        assert!(ctx.event.is_none());
        assert!(ctx.skill_id.is_none());
        assert!(ctx.created_at.is_none());
    }

    #[test]
    fn test_hook_context_chained_builders() {
        let ctx = HookContext::new()
            .with_session("sess-1")
            .with_agent("agent-1")
            .with_turn(5)
            .with_tool("bash", serde_json::json!({"command": "ls"}))
            .with_environment("/work", "host", "staging", "sonnet", "supervised")
            .with_history(
                20,
                5,
                vec!["bash".into(), "file_read".into(), "file_write".into()],
            )
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

    // === ADR-V2-006 §2/§3 envelope-mode unit tests (D120) ===

    #[test]
    fn test_new_auto_populates_created_at() {
        let ctx = HookContext::new();
        let ts = ctx.created_at.as_ref().expect("created_at auto-set");
        // Zulu format: 20 chars like "2026-04-16T14:30:00Z"
        assert_eq!(ts.len(), 20, "unexpected ts {ts:?}");
        assert!(ts.ends_with('Z'), "ts must end with Z: {ts}");
        assert!(
            ts.chars().nth(10) == Some('T'),
            "ts must have T at index 10: {ts}"
        );
    }

    #[test]
    fn test_pre_tool_use_envelope_shape() {
        let ctx = HookContext::new()
            .with_event("PreToolUse")
            .with_session("sess-abc")
            .with_skill_id("threshold-calibration")
            .with_tool(
                "scada_write",
                serde_json::json!({"device_id": "x", "value": 1.0}),
            )
            .with_created_at("2026-04-16T14:30:00Z");
        let json = ctx.to_json();
        assert_eq!(json["event"], "PreToolUse");
        assert_eq!(json["session_id"], "sess-abc");
        assert_eq!(json["skill_id"], "threshold-calibration");
        assert_eq!(json["tool_name"], "scada_write");
        assert_eq!(json["tool_args"]["device_id"], "x");
        assert_eq!(json["created_at"], "2026-04-16T14:30:00Z");
        // Envelope MUST NOT carry non-scope fields like draft_memory_id.
        assert!(json.get("draft_memory_id").is_none());
        assert!(json.get("tool_result").is_none());
    }

    #[test]
    fn test_post_tool_use_envelope_shape() {
        let ctx = HookContext::new()
            .with_event("PostToolUse")
            .with_session("sess-abc")
            .with_skill_id("threshold-calibration")
            .with_tool("scada_write", serde_json::json!({"command": "ls"}))
            .with_result(true, 42)
            .with_created_at("2026-04-16T14:30:05Z");
        let ctx = HookContext {
            tool_result: Some(serde_json::Value::String("ok".into())),
            ..ctx
        };
        let json = ctx.to_json();
        assert_eq!(json["event"], "PostToolUse");
        assert_eq!(json["tool_result"], "ok");
        assert_eq!(json["is_error"], false);
        assert_eq!(json["created_at"], "2026-04-16T14:30:05Z");
        assert!(json.get("draft_memory_id").is_none());
    }

    #[test]
    fn test_stop_envelope_with_absent_optionals_uses_empty_string() {
        let ctx = HookContext::new()
            .with_event("Stop")
            .with_session("sess-abc")
            .with_skill_id("")
            .with_created_at("2026-04-16T14:31:00Z");
        let json = ctx.to_json();
        assert_eq!(json["event"], "Stop");
        assert_eq!(json["draft_memory_id"], "");
        assert_eq!(json["evidence_anchor_id"], "");
        // MUST NOT be null/missing.
        assert!(!json["draft_memory_id"].is_null());
        assert!(!json["evidence_anchor_id"].is_null());
    }

    #[test]
    fn test_env_vars_envelope_mode_always_emits_four_required() {
        let ctx = HookContext::new()
            .with_event("Stop")
            .with_session("sess-abc")
            .with_skill_id("");
        let vars: std::collections::HashMap<String, String> =
            ctx.to_env_vars().into_iter().collect();
        assert_eq!(
            vars.get("GRID_SESSION_ID").map(String::as_str),
            Some("sess-abc")
        );
        assert_eq!(vars.get("GRID_TOOL_NAME").map(String::as_str), Some(""));
        assert_eq!(vars.get("GRID_SKILL_ID").map(String::as_str), Some(""));
        assert_eq!(vars.get("GRID_EVENT").map(String::as_str), Some("Stop"));
    }
}
