//! hooks.yaml configuration types.

use serde::Deserialize;
use std::collections::HashMap;

fn default_timeout() -> u32 {
    10
}

fn default_method() -> String {
    "POST".to_string()
}

/// Top-level hooks.yaml configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct HooksConfig {
    /// Config version (must be 1).
    pub version: u32,
    /// Map of HookPoint name → list of hook entries.
    pub hooks: HashMap<String, Vec<HookEntry>>,
}

/// A single hook entry: a matcher pattern + optional condition + a list of actions.
#[derive(Debug, Clone, Deserialize)]
pub struct HookEntry {
    /// Regex or glob pattern to match tool names. `"*"` matches all.
    pub matcher: String,
    /// Optional condition using PermissionRule syntax, e.g. `"bash(git *)"`.
    /// When present, the hook only triggers if the tool input matches this pattern.
    #[serde(default, rename = "if")]
    pub if_condition: Option<String>,
    /// Actions to execute when the matcher hits.
    pub actions: Vec<HookActionConfig>,
}

/// Failure mode for declarative hooks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// Hook errors are non-fatal (default).
    #[default]
    FailOpen,
    /// Hook errors abort the operation.
    FailClosed,
}

/// Configuration for a single hook action.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum HookActionConfig {
    /// LLM self-evaluation prompt.
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
    },
    /// External script/command execution.
    #[serde(rename = "command")]
    Command {
        command: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
        #[serde(default)]
        failure_mode: FailureMode,
    },
    /// HTTP webhook callback.
    #[serde(rename = "webhook")]
    Webhook {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
        #[serde(default)]
        failure_mode: FailureMode,
    },
    /// WASM component plugin execution.
    #[serde(rename = "wasm")]
    Wasm {
        /// Plugin name (must match an installed plugin's manifest.name).
        plugin: String,
        #[serde(default)]
        failure_mode: FailureMode,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hooks_config() {
        let yaml = r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "bash|shell_execute"
      actions:
        - type: command
          command: "python3 validate.py"
          timeout: 5
          failure_mode: fail_closed
    - matcher: "file_write"
      actions:
        - type: prompt
          prompt: "Check path safety"
  PostToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "bash audit.sh"
          failure_mode: fail_open
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.hooks.len(), 2);

        let pre = &config.hooks["PreToolUse"];
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0].matcher, "bash|shell_execute");
        assert_eq!(pre[0].actions.len(), 1);

        match &pre[0].actions[0] {
            HookActionConfig::Command { command, timeout, failure_mode } => {
                assert_eq!(command, "python3 validate.py");
                assert_eq!(*timeout, 5);
                assert_eq!(*failure_mode, FailureMode::FailClosed);
            }
            _ => panic!("Expected Command action"),
        }

        match &pre[1].actions[0] {
            HookActionConfig::Prompt { prompt, timeout } => {
                assert_eq!(prompt, "Check path safety");
                assert_eq!(*timeout, 10); // default
            }
            _ => panic!("Expected Prompt action"),
        }
    }

    #[test]
    fn test_parse_webhook_action() {
        let yaml = r#"
version: 1
hooks:
  PostToolUse:
    - matcher: "*"
      actions:
        - type: webhook
          url: "https://audit.example.com/api/hook"
          method: POST
          timeout: 5
          failure_mode: fail_open
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        let post = &config.hooks["PostToolUse"];
        match &post[0].actions[0] {
            HookActionConfig::Webhook { url, method, timeout, failure_mode } => {
                assert_eq!(url, "https://audit.example.com/api/hook");
                assert_eq!(method, "POST");
                assert_eq!(*timeout, 5);
                assert_eq!(*failure_mode, FailureMode::FailOpen);
            }
            _ => panic!("Expected Webhook action"),
        }
    }

    #[test]
    fn test_parse_empty_hooks() {
        let yaml = r#"
version: 1
hooks: {}
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn test_default_failure_mode() {
        assert_eq!(FailureMode::default(), FailureMode::FailOpen);
    }

    #[test]
    fn test_parse_wasm_action() {
        let yaml = r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "bash"
      actions:
        - type: wasm
          plugin: "my-security-hook"
          failure_mode: fail_closed
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        let pre = &config.hooks["PreToolUse"];
        match &pre[0].actions[0] {
            HookActionConfig::Wasm { plugin, failure_mode } => {
                assert_eq!(plugin, "my-security-hook");
                assert_eq!(*failure_mode, FailureMode::FailClosed);
            }
            _ => panic!("Expected Wasm action"),
        }
    }

    #[test]
    fn test_parse_wasm_action_default_failure() {
        let yaml = r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: wasm
          plugin: "audit-logger"
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        let pre = &config.hooks["PreToolUse"];
        match &pre[0].actions[0] {
            HookActionConfig::Wasm { plugin, failure_mode } => {
                assert_eq!(plugin, "audit-logger");
                assert_eq!(*failure_mode, FailureMode::FailOpen);
            }
            _ => panic!("Expected Wasm action"),
        }
    }

    #[test]
    fn test_default_timeout() {
        let yaml = r#"
version: 1
hooks:
  PreToolUse:
    - matcher: "*"
      actions:
        - type: command
          command: "test.sh"
"#;
        let config: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        match &config.hooks["PreToolUse"][0].actions[0] {
            HookActionConfig::Command { timeout, failure_mode, .. } => {
                assert_eq!(*timeout, 10); // default
                assert_eq!(*failure_mode, FailureMode::FailOpen); // default
            }
            _ => panic!("Expected Command"),
        }
    }
}
