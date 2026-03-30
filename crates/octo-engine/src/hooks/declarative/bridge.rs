//! DeclarativeHookBridge — bridges hooks.yaml config into HookHandler trait.
//!
//! Registered once per HookPoint, examines the matching entries in `HooksConfig`
//! and executes the configured actions (command, prompt, webhook).

use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;
use tracing::{debug, warn};

use super::command_executor;
use super::config::{FailureMode, HookActionConfig, HooksConfig};
use crate::hooks::{HookAction, HookContext, HookFailureMode, HookHandler, HookPoint};
use crate::providers::Provider;

/// Bridge handler that dispatches declarative hook actions from hooks.yaml.
pub struct DeclarativeHookBridge {
    config: Arc<HooksConfig>,
    /// The HookPoint this bridge instance is registered for.
    hook_point: HookPoint,
    /// Optional LLM provider for prompt-type actions.
    provider: Option<Arc<dyn Provider>>,
    /// Model name for prompt-type actions.
    model: Option<String>,
}

impl DeclarativeHookBridge {
    pub fn new(config: Arc<HooksConfig>, hook_point: HookPoint) -> Self {
        Self {
            config,
            hook_point,
            provider: None,
            model: None,
        }
    }

    /// Create a bridge with LLM provider for prompt-type actions.
    pub fn with_provider(
        mut self,
        provider: Arc<dyn Provider>,
        model: String,
    ) -> Self {
        self.provider = Some(provider);
        self.model = Some(model);
        self
    }

    /// Convert HookPoint to its config key name.
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

    /// Check if a tool name matches a matcher pattern.
    fn matches_tool(matcher: &str, tool_name: &str) -> bool {
        if matcher == "*" {
            return true;
        }
        // Try as regex (pipe-separated alternatives like "bash|shell_execute")
        if let Ok(re) = Regex::new(&format!("^(?:{})$", matcher)) {
            return re.is_match(tool_name);
        }
        // Fallback: exact match
        matcher == tool_name
    }
}

#[async_trait]
impl HookHandler for DeclarativeHookBridge {
    fn name(&self) -> &str {
        "declarative-bridge"
    }

    fn priority(&self) -> u32 {
        500 // Layer 3, runs after builtin (10) and policy (100) handlers
    }

    fn failure_mode(&self) -> HookFailureMode {
        // The bridge itself is FailOpen; individual actions have their own failure modes
        HookFailureMode::FailOpen
    }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        let event_name = self.hook_point_name();
        let entries = match self.config.hooks.get(event_name) {
            Some(entries) => entries,
            None => return Ok(HookAction::Continue),
        };

        let tool_name = ctx.tool_name.as_deref().unwrap_or("");

        for entry in entries {
            if !Self::matches_tool(&entry.matcher, tool_name) {
                continue;
            }

            for action in &entry.actions {
                match action {
                    HookActionConfig::Command {
                        command,
                        timeout,
                        failure_mode,
                    } => {
                        debug!(
                            hook = event_name,
                            command = %command,
                            "Executing declarative command hook"
                        );
                        match command_executor::execute_command(command, ctx, *timeout).await {
                            Ok(decision) => {
                                if decision.is_deny() {
                                    let reason = decision
                                        .reason
                                        .unwrap_or_else(|| "Denied by declarative hook".into());
                                    return Ok(HookAction::Block(reason));
                                }
                                if decision.is_ask() {
                                    // For now, treat "ask" as Block (approval gate integration is AH-D6)
                                    let reason = decision
                                        .reason
                                        .unwrap_or_else(|| "Requires human approval".into());
                                    return Ok(HookAction::Block(reason));
                                }
                                // "allow" — continue to next action
                            }
                            Err(e) => {
                                warn!(
                                    hook = event_name,
                                    command = %command,
                                    error = %e,
                                    "Declarative command hook failed"
                                );
                                if *failure_mode == FailureMode::FailClosed {
                                    return Err(e);
                                }
                                // FailOpen: log and continue
                            }
                        }
                    }
                    HookActionConfig::Prompt {
                        prompt,
                        timeout,
                    } => {
                        if let (Some(ref provider), Some(ref model)) =
                            (&self.provider, &self.model)
                        {
                            debug!(
                                hook = event_name,
                                "Executing declarative prompt hook"
                            );
                            match super::prompt_executor::execute_prompt(
                                prompt, ctx, provider.as_ref(), model, *timeout,
                            )
                            .await
                            {
                                Ok(decision) => {
                                    if decision.is_deny() {
                                        let reason = decision
                                            .reason
                                            .unwrap_or_else(|| "Denied by LLM evaluation".into());
                                        return Ok(HookAction::Block(reason));
                                    }
                                    // "allow" or "ask" — continue
                                }
                                Err(e) => {
                                    warn!(
                                        hook = event_name,
                                        error = %e,
                                        "Prompt hook evaluation failed, treating as allow (fail-open)"
                                    );
                                    // Prompt hooks are fail-open by default
                                }
                            }
                        } else {
                            debug!(
                                hook = event_name,
                                "Skipping prompt action (no LLM provider configured)"
                            );
                        }
                    }
                    HookActionConfig::Webhook {
                        url,
                        method,
                        timeout,
                        failure_mode,
                    } => {
                        debug!(
                            hook = event_name,
                            url = %url,
                            "Executing declarative webhook hook"
                        );
                        match super::webhook_executor::execute_webhook(
                            url, method, ctx, *timeout,
                        )
                        .await
                        {
                            Ok(decision) => {
                                if decision.is_deny() {
                                    let reason = decision
                                        .reason
                                        .unwrap_or_else(|| "Denied by webhook".into());
                                    return Ok(HookAction::Block(reason));
                                }
                            }
                            Err(e) => {
                                warn!(
                                    hook = event_name,
                                    url = %url,
                                    error = %e,
                                    "Webhook hook failed"
                                );
                                if *failure_mode == FailureMode::FailClosed {
                                    return Err(e);
                                }
                                // FailOpen: log and continue
                            }
                        }
                    }
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

    fn make_config(yaml: &str) -> Arc<HooksConfig> {
        Arc::new(serde_yaml::from_str(yaml).unwrap())
    }

    #[test]
    fn test_matches_tool_wildcard() {
        assert!(DeclarativeHookBridge::matches_tool("*", "bash"));
        assert!(DeclarativeHookBridge::matches_tool("*", "file_write"));
        assert!(DeclarativeHookBridge::matches_tool("*", ""));
    }

    #[test]
    fn test_matches_tool_exact() {
        assert!(DeclarativeHookBridge::matches_tool("bash", "bash"));
        assert!(!DeclarativeHookBridge::matches_tool("bash", "file_write"));
    }

    #[test]
    fn test_matches_tool_pipe_alternatives() {
        assert!(DeclarativeHookBridge::matches_tool("bash|shell_execute", "bash"));
        assert!(DeclarativeHookBridge::matches_tool("bash|shell_execute", "shell_execute"));
        assert!(!DeclarativeHookBridge::matches_tool("bash|shell_execute", "file_write"));
    }

    #[test]
    fn test_matches_tool_regex() {
        assert!(DeclarativeHookBridge::matches_tool("file_.*", "file_write"));
        assert!(DeclarativeHookBridge::matches_tool("file_.*", "file_read"));
        assert!(!DeclarativeHookBridge::matches_tool("file_.*", "bash"));
    }

    #[tokio::test]
    async fn test_bridge_no_matching_event() {
        let config = make_config(
            r#"
version: 1
hooks:
  PostToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "echo ok"
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_bridge_no_matching_tool() {
        let config = make_config(
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "file_write"
      actions:
        - type: command
          command: "echo '{\"decision\": \"deny\"}'"
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_bridge_command_allow() {
        let config = make_config(
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "echo '{\"decision\": \"allow\"}'"
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({"command": "ls"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_bridge_command_deny() {
        let config = make_config(
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "bash"
      actions:
        - type: command
          command: "echo '{\"decision\": \"deny\", \"reason\": \"blocked by script\"}'"
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({"command": "rm -rf /"}));
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(
            matches!(result, HookAction::Block(ref r) if r.contains("blocked by script")),
            "got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_bridge_command_fail_open() {
        let config = make_config(
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "exit 1"
          failure_mode: fail_open
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({}));
        // FailOpen: error should be swallowed, returns Continue
        let result = bridge.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_bridge_command_fail_closed() {
        let config = make_config(
            r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "exit 1"
          failure_mode: fail_closed
"#,
        );
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        let ctx = HookContext::new().with_tool("bash", json!({}));
        // FailClosed: error should propagate
        let result = bridge.execute(&ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_metadata() {
        let config = make_config("version: 1\nhooks: {}");
        let bridge = DeclarativeHookBridge::new(config, HookPoint::PreToolUse);
        assert_eq!(bridge.name(), "declarative-bridge");
        assert_eq!(bridge.priority(), 500);
        assert_eq!(bridge.failure_mode(), HookFailureMode::FailOpen);
    }
}
