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
    /// Sandbox profile used for this execution (e.g., "development", "staging", "production")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_profile: Option<String>,
    /// Execution target (e.g., "local", "sandbox:ephemeral:docker")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_target: Option<String>,
    /// Actual sandbox backend used (e.g., "docker", "wasm", "subprocess")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_backend: Option<String>,
    /// Reason for the routing decision
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_reason: Option<String>,
    /// Session container ID (when using session sandbox)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_session_id: Option<String>,
    /// Whether an existing session container was reused (vs newly created)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_container_reused: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_execution() -> ToolExecution {
        ToolExecution {
            id: "exec-1".into(),
            session_id: "sess-1".into(),
            user_id: "user-1".into(),
            tool_name: "bash".into(),
            source: ToolSource::BuiltIn,
            input: serde_json::json!({"cmd": "ls"}),
            output: None,
            status: ExecutionStatus::Success,
            started_at: 1700000000,
            duration_ms: Some(42),
            error: None,
            sandbox_profile: None,
            execution_target: None,
            actual_backend: None,
            routing_reason: None,
            sandbox_session_id: None,
            sandbox_container_reused: None,
        }
    }

    #[test]
    fn test_none_session_fields_omitted_in_json() {
        let exec = make_execution();
        let json = serde_json::to_string(&exec).unwrap();
        assert!(!json.contains("sandbox_session_id"));
        assert!(!json.contains("sandbox_container_reused"));
    }

    #[test]
    fn test_session_fields_present_when_set() {
        let mut exec = make_execution();
        exec.sandbox_session_id = Some("ctr-abc123".into());
        exec.sandbox_container_reused = Some(true);

        let json = serde_json::to_string(&exec).unwrap();
        assert!(json.contains("sandbox_session_id"));
        assert!(json.contains("ctr-abc123"));
        assert!(json.contains("sandbox_container_reused"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_session_fields_roundtrip() {
        let mut exec = make_execution();
        exec.sandbox_session_id = Some("ctr-xyz".into());
        exec.sandbox_container_reused = Some(false);

        let json = serde_json::to_string(&exec).unwrap();
        let restored: ToolExecution = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.sandbox_session_id.as_deref(), Some("ctr-xyz"));
        assert_eq!(restored.sandbox_container_reused, Some(false));
    }

    #[test]
    fn test_deserialize_without_session_fields_defaults_to_none() {
        // Simulate JSON from an older version without the new fields
        let json = serde_json::json!({
            "id": "exec-1",
            "session_id": "sess-1",
            "user_id": "user-1",
            "tool_name": "bash",
            "source": "built_in",
            "input": {},
            "output": null,
            "status": "success",
            "started_at": 0,
            "duration_ms": null,
            "error": null
        });
        let exec: ToolExecution = serde_json::from_value(json).unwrap();
        assert_eq!(exec.sandbox_session_id, None);
        assert_eq!(exec.sandbox_container_reused, None);
    }
}
