//! InProcessHookBridge — in-process hook evaluation for testing and T1 simulation.

use async_trait::async_trait;
use dashmap::DashMap;
use tracing::debug;

use crate::traits::*;

/// In-process HookBridge implementation.
///
/// Evaluates hooks against loaded policies using simple pattern matching.
/// Designed for unit tests and T1 Harness simulation.
pub struct InProcessHookBridge {
    policies: DashMap<String, PolicyRule>,
}

impl InProcessHookBridge {
    pub fn new() -> Self {
        Self {
            policies: DashMap::new(),
        }
    }

    /// Create with pre-loaded policies.
    pub fn with_policies(policies: Vec<PolicyRule>) -> Self {
        let bridge = Self::new();
        for policy in policies {
            bridge.policies.insert(policy.id.clone(), policy);
        }
        bridge
    }

    /// Evaluate policies matching a specific hook type.
    /// Deny-always-wins: if any policy returns Deny, result is Deny (EAASP §10.8).
    fn evaluate_policies(
        &self,
        hook_type: &str,
        tool_name: Option<&str>,
        input: Option<&serde_json::Value>,
    ) -> HookDecision {
        let mut final_decision = HookDecision::Allow;

        for entry in self.policies.iter() {
            let policy = entry.value();
            if !policy.enabled || policy.hook_type != hook_type {
                continue;
            }

            if self.matches_condition(policy, tool_name, input) {
                debug!(
                    policy_id = %policy.id,
                    policy_name = %policy.name,
                    "Policy matched"
                );
                match &policy.action {
                    HookDecision::Deny { .. } => {
                        return policy.action.clone();
                    }
                    HookDecision::Modify { .. } => {
                        final_decision = policy.action.clone();
                    }
                    HookDecision::Allow => {}
                }
            }
        }

        final_decision
    }

    /// Simple condition matching against policy conditions.
    fn matches_condition(
        &self,
        policy: &PolicyRule,
        tool_name: Option<&str>,
        input: Option<&serde_json::Value>,
    ) -> bool {
        let condition = &policy.condition;

        // Match tool_name if specified in condition
        if let Some(expected_tool) = condition.get("tool_name").and_then(|v| v.as_str()) {
            if let Some(actual_tool) = tool_name {
                if actual_tool != expected_tool {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Match pattern in input if specified
        if let Some(pattern) = condition.get("pattern").and_then(|v| v.as_str()) {
            if let Some(input_val) = input {
                let input_str = serde_json::to_string(input_val).unwrap_or_default();
                if !input_str.contains(pattern) {
                    return false;
                }
            }
        }

        // Match always if condition is just `true`
        if condition.is_null() || condition == &serde_json::json!(true) {
            return true;
        }

        true
    }
}

impl Default for InProcessHookBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookBridge for InProcessHookBridge {
    async fn evaluate_pre_tool_call(
        &self,
        _session_id: &str,
        tool_name: &str,
        _tool_id: &str,
        input: &serde_json::Value,
    ) -> anyhow::Result<HookDecision> {
        Ok(self.evaluate_policies("pre_tool_call", Some(tool_name), Some(input)))
    }

    async fn evaluate_post_tool_result(
        &self,
        _session_id: &str,
        tool_name: &str,
        _tool_id: &str,
        _output: &str,
        _is_error: bool,
    ) -> anyhow::Result<HookDecision> {
        Ok(self.evaluate_policies("post_tool_result", Some(tool_name), None))
    }

    async fn evaluate_stop(&self, _session_id: &str) -> anyhow::Result<StopDecision> {
        for entry in self.policies.iter() {
            let policy = entry.value();
            if policy.enabled && policy.hook_type == "stop" {
                if let HookDecision::Deny { reason } = &policy.action {
                    return Ok(StopDecision::Continue {
                        feedback: reason.clone(),
                    });
                }
            }
        }
        Ok(StopDecision::Complete)
    }

    async fn load_policies(&self, policies: Vec<PolicyRule>) -> anyhow::Result<()> {
        for policy in policies {
            self.policies.insert(policy.id.clone(), policy);
        }
        Ok(())
    }

    async fn policy_count(&self) -> usize {
        self.policies.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny_rm_rf_policy() -> PolicyRule {
        PolicyRule {
            id: "p-deny-rm".into(),
            name: "block-rm-rf".into(),
            hook_type: "pre_tool_call".into(),
            scope: "global".into(),
            condition: serde_json::json!({"tool_name": "bash", "pattern": "rm -rf"}),
            action: HookDecision::Deny {
                reason: "destructive command blocked".into(),
            },
            enabled: true,
        }
    }

    fn allow_all_policy() -> PolicyRule {
        PolicyRule {
            id: "p-allow-all".into(),
            name: "allow-everything".into(),
            hook_type: "pre_tool_call".into(),
            scope: "global".into(),
            condition: serde_json::json!(true),
            action: HookDecision::Allow,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn empty_bridge_allows_all() {
        let bridge = InProcessHookBridge::new();
        let result = bridge
            .evaluate_pre_tool_call("s-1", "bash", "t-1", &serde_json::json!({"command": "ls"}))
            .await
            .unwrap();
        assert_eq!(result, HookDecision::Allow);
    }

    #[tokio::test]
    async fn deny_policy_blocks_matching_tool() {
        let bridge = InProcessHookBridge::with_policies(vec![deny_rm_rf_policy()]);
        let result = bridge
            .evaluate_pre_tool_call(
                "s-1",
                "bash",
                "t-1",
                &serde_json::json!({"command": "rm -rf /"}),
            )
            .await
            .unwrap();
        assert!(matches!(result, HookDecision::Deny { .. }));
    }

    #[tokio::test]
    async fn deny_policy_allows_non_matching_tool() {
        let bridge = InProcessHookBridge::with_policies(vec![deny_rm_rf_policy()]);
        let result = bridge
            .evaluate_pre_tool_call(
                "s-1",
                "bash",
                "t-1",
                &serde_json::json!({"command": "ls -la"}),
            )
            .await
            .unwrap();
        assert_eq!(result, HookDecision::Allow);
    }

    #[tokio::test]
    async fn deny_always_wins() {
        let bridge =
            InProcessHookBridge::with_policies(vec![allow_all_policy(), deny_rm_rf_policy()]);
        let result = bridge
            .evaluate_pre_tool_call(
                "s-1",
                "bash",
                "t-1",
                &serde_json::json!({"command": "rm -rf /tmp"}),
            )
            .await
            .unwrap();
        assert!(matches!(result, HookDecision::Deny { .. }));
    }

    #[tokio::test]
    async fn load_policies_dynamically() {
        let bridge = InProcessHookBridge::new();
        assert_eq!(bridge.policy_count().await, 0);
        bridge
            .load_policies(vec![deny_rm_rf_policy()])
            .await
            .unwrap();
        assert_eq!(bridge.policy_count().await, 1);
    }

    #[tokio::test]
    async fn stop_decision_with_continue_policy() {
        let stop_policy = PolicyRule {
            id: "p-stop".into(),
            name: "force-continue".into(),
            hook_type: "stop".into(),
            scope: "global".into(),
            condition: serde_json::json!(true),
            action: HookDecision::Deny {
                reason: "task incomplete".into(),
            },
            enabled: true,
        };
        let bridge = InProcessHookBridge::with_policies(vec![stop_policy]);
        let result = bridge.evaluate_stop("s-1").await.unwrap();
        assert!(matches!(result, StopDecision::Continue { .. }));
    }

    #[tokio::test]
    async fn disabled_policy_is_skipped() {
        let mut policy = deny_rm_rf_policy();
        policy.enabled = false;
        let bridge = InProcessHookBridge::with_policies(vec![policy]);
        let result = bridge
            .evaluate_pre_tool_call(
                "s-1",
                "bash",
                "t-1",
                &serde_json::json!({"command": "rm -rf /"}),
            )
            .await
            .unwrap();
        assert_eq!(result, HookDecision::Allow);
    }

    #[tokio::test]
    async fn different_tool_name_does_not_match() {
        let bridge = InProcessHookBridge::with_policies(vec![deny_rm_rf_policy()]);
        let result = bridge
            .evaluate_pre_tool_call(
                "s-1",
                "read_file",
                "t-1",
                &serde_json::json!({"path": "/etc/passwd"}),
            )
            .await
            .unwrap();
        assert_eq!(result, HookDecision::Allow);
    }
}
