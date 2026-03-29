//! policies.yaml configuration types.

use serde::Deserialize;

/// Top-level policies.yaml configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    pub version: u32,
    pub policies: Vec<PolicyEntry>,
}

/// A single policy entry with name, enabled flag, hooks, matcher, and rules.
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyEntry {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Which hook points this policy applies to (e.g., ["PreToolUse"]).
    pub hooks: Vec<String>,
    /// Tool name matcher (regex or "*").
    pub matcher: String,
    /// Optional condition expression (e.g., "context.sandbox_profile == 'production'").
    #[serde(default)]
    pub condition: Option<String>,
    /// Policy rules to evaluate.
    pub rules: Vec<PolicyRule>,
}

/// A single policy rule.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PolicyRule {
    /// Deny specific paths.
    DenyPaths {
        deny_paths: Vec<String>,
    },
    /// Deny path patterns (glob).
    DenyPatterns {
        deny_patterns: Vec<String>,
    },
    /// Deny specific commands.
    DenyCommands {
        deny_commands: Vec<String>,
    },
    /// Require approval for commands matching patterns.
    RequireApproval {
        require_approval: Vec<String>,
    },
    /// Deny specific tools entirely.
    DenyTools {
        deny_tools: Vec<String>,
        #[serde(default)]
        message: Option<String>,
    },
    /// Rate limit per tool.
    RateLimit {
        tool: String,
        max_per_minute: u32,
    },
}

fn default_true() -> bool {
    true
}

/// Load and parse a policies.yaml file.
pub fn load_policies_config(path: &std::path::Path) -> anyhow::Result<PolicyConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: PolicyConfig = serde_yaml::from_str(&content)?;
    if config.version != 1 {
        anyhow::bail!("Unsupported policies.yaml version: {}", config.version);
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_policies_config() {
        let yaml = r#"
version: 1
policies:
  - name: path_safety
    enabled: true
    hooks: [PreToolUse]
    matcher: "file_write|file_edit|file_read"
    rules:
      - deny_paths: ["/etc", "/sys", "/proc"]
      - deny_patterns: ["**/credentials*", "**/.env*"]

  - name: command_safety
    hooks: [PreToolUse]
    matcher: "bash|shell_execute"
    rules:
      - deny_commands: ["rm -rf /", "mkfs"]
      - require_approval: ["sudo *", "docker run *"]

  - name: production_lockdown
    hooks: [PreToolUse]
    matcher: "*"
    condition: "context.sandbox_profile == 'production'"
    rules:
      - deny_tools: ["file_write", "file_edit", "bash"]
        message: "Production sandbox: write operations blocked"
"#;
        let config: PolicyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.policies.len(), 3);
        assert_eq!(config.policies[0].name, "path_safety");
        assert!(config.policies[0].enabled);
        assert_eq!(config.policies[0].hooks, vec!["PreToolUse"]);
        assert_eq!(config.policies[2].condition.as_deref(), Some("context.sandbox_profile == 'production'"));
    }

    #[test]
    fn test_parse_rate_limit_rule() {
        let yaml = r#"
version: 1
policies:
  - name: rate_limits
    hooks: [PreToolUse]
    matcher: "*"
    rules:
      - tool: "bash"
        max_per_minute: 30
      - tool: "*"
        max_per_minute: 120
"#;
        let config: PolicyConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.policies[0].rules.len(), 2);
        match &config.policies[0].rules[0] {
            PolicyRule::RateLimit { tool, max_per_minute } => {
                assert_eq!(tool, "bash");
                assert_eq!(*max_per_minute, 30);
            }
            _ => panic!("Expected RateLimit"),
        }
    }

    #[test]
    fn test_default_enabled() {
        let yaml = r#"
version: 1
policies:
  - name: test
    hooks: [PreToolUse]
    matcher: "*"
    rules:
      - deny_paths: ["/tmp"]
"#;
        let config: PolicyConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.policies[0].enabled); // default true
    }
}
