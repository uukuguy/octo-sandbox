//! SecurityPolicyHandler — PreToolUse hook that enforces forbidden path checks.
//!
//! Bridges the existing `SecurityPolicy::check_path()` logic into the hook
//! system so that path safety is enforced uniformly through the hook chain.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use crate::hooks::{HookAction, HookContext, HookFailureMode, HookHandler};
use crate::security::SecurityPolicy;

/// PreToolUse hook handler that checks tool inputs against `SecurityPolicy`.
///
/// Currently checks:
/// - `path` field in tool_input against `SecurityPolicy::forbidden_paths`
/// - `command` field against `SecurityPolicy::check_command()` + risk assessment
pub struct SecurityPolicyHandler {
    policy: Arc<SecurityPolicy>,
}

impl SecurityPolicyHandler {
    pub fn new(policy: Arc<SecurityPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl HookHandler for SecurityPolicyHandler {
    fn name(&self) -> &str {
        "security-policy"
    }

    fn priority(&self) -> u32 {
        10 // High priority — runs early in the chain
    }

    fn failure_mode(&self) -> HookFailureMode {
        HookFailureMode::FailClosed
    }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        let Some(ref input) = ctx.tool_input else {
            return Ok(HookAction::Continue);
        };

        // Check file path in tool input
        if let Some(path_str) = input.get("path").and_then(|v| v.as_str()) {
            let path = Path::new(path_str);
            if let Err(reason) = self.policy.check_path(path) {
                return Ok(HookAction::Block(reason));
            }
        }

        // Check command in tool input (for bash/shell tools)
        if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
            if let Err(reason) = self.policy.check_command(command) {
                return Ok(HookAction::Block(reason));
            }
            // Also check if high-risk commands should be blocked
            if self.policy.block_high_risk_commands {
                let risk = self.policy.assess_command_risk(command);
                if risk == crate::security::CommandRiskLevel::High {
                    return Ok(HookAction::Block(format!(
                        "High-risk command blocked by security policy: {}",
                        command.chars().take(80).collect::<String>()
                    )));
                }
            }
        }

        Ok(HookAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_policy_with_forbidden(paths: Vec<&str>) -> Arc<SecurityPolicy> {
        let mut policy = SecurityPolicy::default();
        policy.forbidden_paths = paths.into_iter().map(String::from).collect();
        // Disable workspace_only for test simplicity
        policy.workspace_only = false;
        Arc::new(policy)
    }

    #[tokio::test]
    async fn test_blocks_forbidden_path() {
        let policy = make_policy_with_forbidden(vec!["/etc", "/sys"]);
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/etc/passwd", "content": "x"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Block(ref r) if r.contains("forbidden")),
            "expected Block, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_allows_safe_path() {
        let policy = make_policy_with_forbidden(vec!["/etc"]);
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/home/user/project/main.rs", "content": "x"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Continue),
            "expected Continue, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_blocks_high_risk_command() {
        let policy = Arc::new(SecurityPolicy::default());
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "rm -rf /"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Block(ref r) if r.contains("High-risk")),
            "expected Block for high-risk, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_allows_low_risk_command() {
        let policy = Arc::new(SecurityPolicy::default());
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "ls -la"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Continue),
            "expected Continue for ls, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_no_input_continues() {
        let policy = Arc::new(SecurityPolicy::default());
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new().with_session("s1");
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_handler_metadata() {
        let policy = Arc::new(SecurityPolicy::default());
        let handler = SecurityPolicyHandler::new(policy);
        assert_eq!(handler.name(), "security-policy");
        assert_eq!(handler.priority(), 10);
        assert_eq!(handler.failure_mode(), HookFailureMode::FailClosed);
    }

    #[tokio::test]
    async fn test_readonly_blocks_command() {
        use crate::security::AutonomyLevel;
        let policy = Arc::new(
            SecurityPolicy::default().with_autonomy(AutonomyLevel::ReadOnly),
        );
        let handler = SecurityPolicyHandler::new(policy);
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "echo hello"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Block(_)),
            "ReadOnly should block even safe commands, got {:?}",
            result
        );
    }
}
