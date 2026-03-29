//! PolicyEngineBridge — bridges policies.yaml rules into HookHandler trait.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};

use super::config::PolicyConfig;
use super::matcher::PolicyMatcher;
use crate::hooks::{HookAction, HookContext, HookFailureMode, HookHandler, HookPoint};

/// Bridge handler that evaluates policy rules from policies.yaml.
pub struct PolicyEngineBridge {
    config: Arc<PolicyConfig>,
    hook_point: HookPoint,
}

impl PolicyEngineBridge {
    pub fn new(config: Arc<PolicyConfig>, hook_point: HookPoint) -> Self {
        Self { config, hook_point }
    }

    fn hook_point_name(&self) -> &'static str {
        match self.hook_point {
            HookPoint::PreToolUse => "PreToolUse",
            HookPoint::PostToolUse => "PostToolUse",
            HookPoint::PreTask => "PreTask",
            HookPoint::PostTask => "PostTask",
            HookPoint::SessionStart => "SessionStart",
            HookPoint::SessionEnd => "SessionEnd",
            HookPoint::ContextDegraded => "ContextDegraded",
            HookPoint::LoopTurnStart => "LoopTurnStart",
            HookPoint::LoopTurnEnd => "LoopTurnEnd",
            HookPoint::AgentRoute => "AgentRoute",
            HookPoint::SkillsActivated => "SkillsActivated",
            HookPoint::SkillDeactivated => "SkillDeactivated",
            HookPoint::SkillScriptStarted => "SkillScriptStarted",
            HookPoint::ToolConstraintViolated => "ToolConstraintViolated",
        }
    }

    /// Check if a tool name matches the policy matcher pattern.
    fn matches_tool(matcher: &str, tool_name: &str) -> bool {
        if matcher == "*" {
            return true;
        }
        if let Ok(re) = regex::Regex::new(&format!("^(?:{})$", matcher)) {
            return re.is_match(tool_name);
        }
        matcher == tool_name
    }
}

#[async_trait]
impl HookHandler for PolicyEngineBridge {
    fn name(&self) -> &str {
        "policy-engine"
    }

    fn priority(&self) -> u32 {
        100 // Layer 2, between builtin (10) and declarative (500)
    }

    fn failure_mode(&self) -> HookFailureMode {
        HookFailureMode::FailClosed // Policy violations are security-critical
    }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        let event_name = self.hook_point_name();
        let tool_name = ctx.tool_name.as_deref().unwrap_or("");

        for policy in &self.config.policies {
            // Skip disabled policies
            if !policy.enabled {
                continue;
            }

            // Check if this policy applies to the current hook point
            if !policy.hooks.iter().any(|h| h == event_name) {
                continue;
            }

            // Check tool matcher
            if !Self::matches_tool(&policy.matcher, tool_name) {
                continue;
            }

            // Check condition if present
            if let Some(ref condition) = policy.condition {
                if !PolicyMatcher::evaluate_condition(condition, ctx) {
                    debug!(
                        policy = %policy.name,
                        condition = %condition,
                        "Policy condition not met, skipping"
                    );
                    continue;
                }
            }

            // Evaluate all rules
            for rule in &policy.rules {
                if let Some(reason) = PolicyMatcher::evaluate(rule, ctx) {
                    warn!(
                        policy = %policy.name,
                        tool = tool_name,
                        reason = %reason,
                        "Policy rule violated"
                    );
                    return Ok(HookAction::Block(format!(
                        "[policy:{}] {}",
                        policy.name, reason
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

    fn make_config(yaml: &str) -> Arc<PolicyConfig> {
        Arc::new(serde_yaml::from_str(yaml).unwrap())
    }

    #[tokio::test]
    async fn test_policy_blocks_forbidden_path() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: path_safety
    hooks: [PreToolUse]
    matcher: "file_write|file_edit"
    rules:
      - deny_paths: ["/etc", "/sys"]
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/etc/passwd"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Block(ref r) if r.contains("path_safety")),
            "got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_policy_allows_safe_tool() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: path_safety
    hooks: [PreToolUse]
    matcher: "file_write"
    rules:
      - deny_paths: ["/etc"]
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "ls"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_policy_condition_production_lockdown() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: prod_lockdown
    hooks: [PreToolUse]
    matcher: "*"
    condition: "context.sandbox_profile == 'production'"
    rules:
      - deny_tools: ["bash", "file_write"]
        message: "Production mode"
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);

        // In production: should block
        let ctx = HookContext::new()
            .with_tool("bash", json!({}))
            .with_environment("/work", "docker", "production", "sonnet", "restricted");
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Block(_)));

        // In development: should allow
        let ctx = HookContext::new()
            .with_tool("bash", json!({}))
            .with_environment("/work", "host", "development", "sonnet", "full");
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_policy_disabled_skipped() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: disabled_policy
    enabled: false
    hooks: [PreToolUse]
    matcher: "*"
    rules:
      - deny_paths: ["/"]
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/etc/passwd"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_policy_wrong_hook_point_skipped() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: post_only
    hooks: [PostToolUse]
    matcher: "*"
    rules:
      - deny_paths: ["/etc"]
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/etc/passwd"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_policy_command_deny() {
        let config = make_config(
            r#"
version: 1
policies:
  - name: cmd_safety
    hooks: [PreToolUse]
    matcher: "bash"
    rules:
      - deny_commands: ["rm -rf /", "mkfs"]
"#,
        );
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "rm -rf / --no-preserve-root"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Block(_)));
    }

    #[test]
    fn test_bridge_metadata() {
        let config = make_config("version: 1\npolicies: []");
        let bridge = PolicyEngineBridge::new(config, HookPoint::PreToolUse);
        assert_eq!(bridge.name(), "policy-engine");
        assert_eq!(bridge.priority(), 100);
        assert_eq!(bridge.failure_mode(), HookFailureMode::FailClosed);
    }
}
