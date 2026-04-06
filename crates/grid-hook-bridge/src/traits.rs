//! HookBridge trait — abstraction for hook evaluation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Hook evaluation decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookDecision {
    Allow,
    Deny { reason: String },
    Modify { transformed_input: serde_json::Value },
}

/// Stop hook decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopDecision {
    Complete,
    Continue { feedback: String },
}

/// Hook event types for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolCall {
        session_id: String,
        tool_name: String,
        tool_id: String,
        input: serde_json::Value,
    },
    PostToolResult {
        session_id: String,
        tool_name: String,
        tool_id: String,
        output: String,
        is_error: bool,
    },
    Stop {
        session_id: String,
        reason: String,
    },
    SessionStart {
        session_id: String,
        user_id: String,
        user_role: String,
        org_unit: String,
    },
    SessionEnd {
        session_id: String,
        reason: String,
    },
}

/// Policy rule for hook evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    /// "pre_tool_call" | "post_tool_result" | "stop" | "session_start" | "session_end"
    pub hook_type: String,
    /// "global" | "session" | "skill"
    pub scope: String,
    /// JSON condition expression.
    pub condition: serde_json::Value,
    /// Decision when condition matches.
    pub action: HookDecision,
    pub enabled: bool,
}

/// HookBridge trait — the core abstraction.
///
/// Implementations:
/// - `InProcessHookBridge` — in-process evaluation (tests, T1 simulation)
/// - `GrpcHookBridge` — gRPC client to external sidecar (T2/T3 production)
#[async_trait]
pub trait HookBridge: Send + Sync {
    /// Evaluate a pre-tool-call hook.
    async fn evaluate_pre_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_id: &str,
        input: &serde_json::Value,
    ) -> anyhow::Result<HookDecision>;

    /// Evaluate a post-tool-result hook.
    async fn evaluate_post_tool_result(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_id: &str,
        output: &str,
        is_error: bool,
    ) -> anyhow::Result<HookDecision>;

    /// Evaluate a stop hook.
    async fn evaluate_stop(&self, session_id: &str) -> anyhow::Result<StopDecision>;

    /// Load/update policies.
    async fn load_policies(&self, policies: Vec<PolicyRule>) -> anyhow::Result<()>;

    /// Get current policy count.
    async fn policy_count(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_decision_serialization() {
        let allow = HookDecision::Allow;
        let json = serde_json::to_string(&allow).unwrap();
        let restored: HookDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, HookDecision::Allow);

        let deny = HookDecision::Deny {
            reason: "blocked".into(),
        };
        let json = serde_json::to_string(&deny).unwrap();
        assert!(json.contains("blocked"));
    }

    #[test]
    fn policy_rule_creation() {
        let rule = PolicyRule {
            id: "p-1".into(),
            name: "block-rm-rf".into(),
            hook_type: "pre_tool_call".into(),
            scope: "global".into(),
            condition: serde_json::json!({"tool_name": "bash", "pattern": "rm -rf"}),
            action: HookDecision::Deny {
                reason: "destructive command blocked".into(),
            },
            enabled: true,
        };
        assert!(rule.enabled);
        assert_eq!(rule.hook_type, "pre_tool_call");
    }

    #[test]
    fn hook_event_variants() {
        let event = HookEvent::PreToolCall {
            session_id: "s-1".into(),
            tool_name: "bash".into(),
            tool_id: "t-1".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PreToolCall"));

        let stop = HookEvent::Stop {
            session_id: "s-1".into(),
            reason: "max_turns".into(),
        };
        let json = serde_json::to_string(&stop).unwrap();
        assert!(json.contains("max_turns"));
    }
}
