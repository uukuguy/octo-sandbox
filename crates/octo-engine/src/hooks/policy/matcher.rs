//! Policy rule matcher — evaluates policy rules against HookContext.

use crate::hooks::HookContext;
use super::config::PolicyRule;

/// Evaluates policy rules against a HookContext.
pub struct PolicyMatcher;

impl PolicyMatcher {
    /// Evaluate a single policy rule against the context.
    ///
    /// Returns `Some(reason)` if the rule blocks the operation, `None` if it passes.
    pub fn evaluate(rule: &PolicyRule, ctx: &HookContext) -> Option<String> {
        match rule {
            PolicyRule::DenyPaths { deny_paths } => {
                Self::check_deny_paths(deny_paths, ctx)
            }
            PolicyRule::DenyPatterns { deny_patterns } => {
                Self::check_deny_patterns(deny_patterns, ctx)
            }
            PolicyRule::DenyCommands { deny_commands } => {
                Self::check_deny_commands(deny_commands, ctx)
            }
            PolicyRule::RequireApproval { require_approval } => {
                Self::check_require_approval(require_approval, ctx)
            }
            PolicyRule::DenyTools { deny_tools, message } => {
                Self::check_deny_tools(deny_tools, message.as_deref(), ctx)
            }
            PolicyRule::RateLimit { .. } => {
                // Rate limiting needs stateful tracking; deferred to future enhancement
                None
            }
        }
    }

    /// Check if tool input contains a path that starts with any denied prefix.
    fn check_deny_paths(deny_paths: &[String], ctx: &HookContext) -> Option<String> {
        let path = Self::extract_path(ctx)?;
        for denied in deny_paths {
            let expanded = Self::expand_home(denied);
            if path.starts_with(&expanded) {
                return Some(format!(
                    "Path '{}' is denied by policy (prefix: {})",
                    path, denied
                ));
            }
        }
        None
    }

    /// Check if tool input contains a path matching any denied glob pattern.
    fn check_deny_patterns(deny_patterns: &[String], ctx: &HookContext) -> Option<String> {
        let path = Self::extract_path(ctx)?;
        for pattern in deny_patterns {
            // Simple glob matching: ** = any path segment, * = any chars in segment
            let regex_pattern = pattern
                .replace(".", r"\.")
                .replace("**", "§§") // placeholder
                .replace("*", "[^/]*")
                .replace("§§", ".*");
            if let Ok(re) = regex::Regex::new(&format!("(?:^|/){}$", regex_pattern)) {
                if re.is_match(&path) {
                    return Some(format!(
                        "Path '{}' matches denied pattern '{}'",
                        path, pattern
                    ));
                }
            }
        }
        None
    }

    /// Check if tool input contains a command matching any denied command.
    fn check_deny_commands(deny_commands: &[String], ctx: &HookContext) -> Option<String> {
        let command = Self::extract_command(ctx)?;
        let cmd_lower = command.to_lowercase();
        for denied in deny_commands {
            if cmd_lower.contains(&denied.to_lowercase()) {
                return Some(format!(
                    "Command contains denied pattern '{}': {}",
                    denied,
                    &command[..command.len().min(80)]
                ));
            }
        }
        None
    }

    /// Check if tool input command matches any require_approval pattern.
    fn check_require_approval(patterns: &[String], ctx: &HookContext) -> Option<String> {
        let command = Self::extract_command(ctx)?;
        for pattern in patterns {
            let regex_pattern = pattern
                .replace(".", r"\.")
                .replace("*", ".*");
            if let Ok(re) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
                if re.is_match(&command) {
                    return Some(format!(
                        "Command requires approval (pattern: '{}'): {}",
                        pattern,
                        &command[..command.len().min(80)]
                    ));
                }
            }
        }
        None
    }

    /// Check if the current tool is in the deny list.
    fn check_deny_tools(
        deny_tools: &[String],
        message: Option<&str>,
        ctx: &HookContext,
    ) -> Option<String> {
        let tool_name = ctx.tool_name.as_deref()?;
        if deny_tools.iter().any(|t| t == tool_name) {
            let msg = message.unwrap_or("Tool denied by policy");
            return Some(format!("{}: {}", msg, tool_name));
        }
        None
    }

    /// Check if a simple condition expression matches the context.
    ///
    /// Supports: `context.field == 'value'` and `context.field != 'value'`.
    pub fn evaluate_condition(condition: &str, ctx: &HookContext) -> bool {
        // Parse simple conditions: "context.field == 'value'" or "context.field != 'value'"
        let parts: Vec<&str> = if condition.contains("!=") {
            let p: Vec<&str> = condition.splitn(2, "!=").collect();
            if p.len() != 2 { return false; }
            let field = p[0].trim();
            let value = p[1].trim().trim_matches('\'').trim_matches('"');
            let actual = Self::resolve_context_field(field, ctx);
            return actual.as_deref() != Some(value);
        } else if condition.contains("==") {
            condition.splitn(2, "==").collect()
        } else {
            return false;
        };

        if parts.len() != 2 {
            return false;
        }

        let field = parts[0].trim();
        let value = parts[1].trim().trim_matches('\'').trim_matches('"');
        let actual = Self::resolve_context_field(field, ctx);
        actual.as_deref() == Some(value)
    }

    /// Resolve a dotted field reference against HookContext.
    fn resolve_context_field(field: &str, ctx: &HookContext) -> Option<String> {
        let field = field.strip_prefix("context.").unwrap_or(field);
        match field {
            "sandbox_profile" => ctx.sandbox_profile.clone(),
            "sandbox_mode" => ctx.sandbox_mode.clone(),
            "autonomy_level" => ctx.autonomy_level.clone(),
            "model" => ctx.model.clone(),
            "working_dir" => ctx.working_dir.clone(),
            "tool_name" => ctx.tool_name.clone(),
            "session_id" => ctx.session_id.clone(),
            _ => None,
        }
    }

    /// Extract a path from tool_input.
    fn extract_path(ctx: &HookContext) -> Option<String> {
        ctx.tool_input
            .as_ref()?
            .get("path")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Extract a command from tool_input.
    fn extract_command(ctx: &HookContext) -> Option<String> {
        ctx.tool_input
            .as_ref()?
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    fn expand_home(path: &str) -> String {
        if path.starts_with("~/") || path == "~" {
            if let Some(home) = dirs::home_dir() {
                return path.replacen("~", &home.display().to_string(), 1);
            }
        }
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deny_paths_blocks() {
        let rule = PolicyRule::DenyPaths {
            deny_paths: vec!["/etc".into(), "/sys".into()],
        };
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/etc/passwd"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_some());
    }

    #[test]
    fn test_deny_paths_allows() {
        let rule = PolicyRule::DenyPaths {
            deny_paths: vec!["/etc".into()],
        };
        let ctx = HookContext::new()
            .with_tool("file_write", json!({"path": "/home/user/file.txt"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }

    #[test]
    fn test_deny_patterns_blocks() {
        let rule = PolicyRule::DenyPatterns {
            deny_patterns: vec!["**/.env*".into(), "**/credentials*".into()],
        };
        let ctx = HookContext::new()
            .with_tool("file_read", json!({"path": "/home/user/project/.env.local"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_some());
    }

    #[test]
    fn test_deny_patterns_allows() {
        let rule = PolicyRule::DenyPatterns {
            deny_patterns: vec!["**/.env*".into()],
        };
        let ctx = HookContext::new()
            .with_tool("file_read", json!({"path": "/home/user/src/main.rs"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }

    #[test]
    fn test_deny_commands_blocks() {
        let rule = PolicyRule::DenyCommands {
            deny_commands: vec!["rm -rf /".into(), "mkfs".into()],
        };
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "rm -rf / --no-preserve-root"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_some());
    }

    #[test]
    fn test_deny_commands_allows() {
        let rule = PolicyRule::DenyCommands {
            deny_commands: vec!["rm -rf /".into()],
        };
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "ls -la"}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }

    #[test]
    fn test_require_approval_matches() {
        let rule = PolicyRule::RequireApproval {
            require_approval: vec!["sudo *".into(), "docker run *".into()],
        };
        let ctx = HookContext::new()
            .with_tool("bash", json!({"command": "sudo apt install vim"}));
        let result = PolicyMatcher::evaluate(&rule, &ctx);
        assert!(result.is_some());
        assert!(result.unwrap().contains("requires approval"));
    }

    #[test]
    fn test_deny_tools_blocks() {
        let rule = PolicyRule::DenyTools {
            deny_tools: vec!["file_write".into(), "bash".into()],
            message: Some("Production lockdown".into()),
        };
        let ctx = HookContext::new()
            .with_tool("bash", json!({}));
        let result = PolicyMatcher::evaluate(&rule, &ctx);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Production lockdown"));
    }

    #[test]
    fn test_deny_tools_allows() {
        let rule = PolicyRule::DenyTools {
            deny_tools: vec!["bash".into()],
            message: None,
        };
        let ctx = HookContext::new()
            .with_tool("file_read", json!({}));
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }

    #[test]
    fn test_condition_equals() {
        let ctx = HookContext::new()
            .with_environment("/tmp", "docker", "production", "sonnet", "supervised");
        assert!(PolicyMatcher::evaluate_condition(
            "context.sandbox_profile == 'production'",
            &ctx
        ));
        assert!(!PolicyMatcher::evaluate_condition(
            "context.sandbox_profile == 'development'",
            &ctx
        ));
    }

    #[test]
    fn test_condition_not_equals() {
        let ctx = HookContext::new()
            .with_environment("/tmp", "host", "development", "sonnet", "full");
        assert!(PolicyMatcher::evaluate_condition(
            "context.sandbox_mode != 'docker'",
            &ctx
        ));
        assert!(!PolicyMatcher::evaluate_condition(
            "context.sandbox_mode != 'host'",
            &ctx
        ));
    }

    #[test]
    fn test_no_tool_input_passes() {
        let rule = PolicyRule::DenyPaths {
            deny_paths: vec!["/etc".into()],
        };
        let ctx = HookContext::new().with_session("s1");
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }

    #[test]
    fn test_rate_limit_passes() {
        let rule = PolicyRule::RateLimit {
            tool: "bash".into(),
            max_per_minute: 30,
        };
        let ctx = HookContext::new().with_tool("bash", json!({}));
        // Rate limiting is stateless in matcher, always passes
        assert!(PolicyMatcher::evaluate(&rule, &ctx).is_none());
    }
}
