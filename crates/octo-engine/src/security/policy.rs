//! Security policy for tool execution.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::ActionTracker;

/// How much autonomy the agent has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutonomyLevel {
    /// Read-only: can observe but not act.
    ReadOnly,
    /// Supervised: acts but requires approval for risky operations.
    #[default]
    Supervised,
    /// Full: autonomous execution within policy bounds.
    Full,
}

impl AutonomyLevel {
    /// Check if this level allows tool execution.
    pub fn allows_execution(&self) -> bool {
        !matches!(self, AutonomyLevel::ReadOnly)
    }

    /// Check if this level requires approval for medium-risk operations.
    pub fn requires_approval(&self, risk: CommandRiskLevel) -> bool {
        match (self, risk) {
            (AutonomyLevel::ReadOnly, _) => true,
            (AutonomyLevel::Supervised, CommandRiskLevel::Medium | CommandRiskLevel::High) => true,
            (AutonomyLevel::Full, _) => false,
            _ => false,
        }
    }
}

/// Risk score for command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandRiskLevel {
    Low,
    Medium,
    High,
}

/// Security policy enforced on all tool executions.
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    /// Autonomy level for this session.
    pub autonomy: AutonomyLevel,
    /// Allowed workspace directory.
    pub workspace_dir: PathBuf,
    /// Whether to restrict all operations to workspace only.
    pub workspace_only: bool,
    /// Whitelist of allowed commands (empty = allow all).
    pub allowed_commands: Vec<String>,
    /// Blacklist of forbidden paths.
    pub forbidden_paths: Vec<String>,
    /// Maximum actions per hour.
    pub max_actions_per_hour: u32,
    /// Maximum cost per day in cents.
    pub max_cost_per_day_cents: u32,
    /// Whether to require approval for medium-risk commands.
    pub require_approval_for_medium_risk: bool,
    /// Whether to block high-risk commands entirely.
    pub block_high_risk_commands: bool,
    /// Action tracker for rate limiting.
    pub tracker: ActionTracker,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            autonomy: AutonomyLevel::Supervised,
            workspace_dir: PathBuf::from("."),
            workspace_only: true,
            allowed_commands: vec![
                "git".into(),
                "npm".into(),
                "cargo".into(),
                "ls".into(),
                "cat".into(),
                "grep".into(),
                "find".into(),
                "echo".into(),
                "pwd".into(),
                "wc".into(),
                "head".into(),
                "tail".into(),
            ],
            forbidden_paths: vec![
                // System directories (blocked even when workspace_only=false)
                "/etc".into(),
                "/root".into(),
                "/usr".into(),
                "/bin".into(),
                "/sbin".into(),
                "/lib".into(),
                "/opt".into(),
                "/boot".into(),
                "/dev".into(),
                "/proc".into(),
                "/sys".into(),
                "/var".into(),
                "/tmp".into(),
                // Sensitive dotfiles
                "~/.ssh".into(),
                "~/.gnupg".into(),
                "~/.aws".into(),
                "~/.config".into(),
            ],
            max_actions_per_hour: 20,
            max_cost_per_day_cents: 500,
            require_approval_for_medium_risk: true,
            block_high_risk_commands: true,
            tracker: ActionTracker::new(),
        }
    }
}

impl SecurityPolicy {
    /// Create a new security policy with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a policy with custom workspace.
    pub fn with_workspace(mut self, workspace: PathBuf) -> Self {
        self.workspace_dir = workspace;
        self
    }

    /// Create a policy with custom autonomy level.
    pub fn with_autonomy(mut self, autonomy: AutonomyLevel) -> Self {
        self.autonomy = autonomy;
        self
    }

    /// Check if a command is allowed.
    pub fn check_command(&self, command: &str) -> Result<(), String> {
        // Check autonomy level
        if !self.autonomy.allows_execution() {
            return Err("Autonomy level is ReadOnly - no execution allowed".into());
        }

        // Check command whitelist
        if !self.allowed_commands.is_empty() {
            let cmd_base = command
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split('/')
                .last()
                .unwrap_or("");

            if !self.allowed_commands.iter().any(|c| c == cmd_base) {
                return Err(format!(
                    "Command '{}' is not in the allowed commands list",
                    cmd_base
                ));
            }
        }

        Ok(())
    }

    /// Check if a path is allowed.
    pub fn check_path(&self, path: &Path) -> Result<(), String> {
        // Expand home directory
        let expanded = if path.starts_with("~") {
            if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(path.strip_prefix("~").unwrap_or(path))
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        // Check workspace restriction
        if self.workspace_only {
            let workspace = self
                .workspace_dir
                .canonicalize()
                .unwrap_or_else(|_| self.workspace_dir.clone());
            // For new files that don't exist yet, canonicalize the parent directory
            let expanded_canonical = expanded.canonicalize().unwrap_or_else(|_| {
                if let Some(parent) = expanded.parent() {
                    parent
                        .canonicalize()
                        .map(|p| p.join(expanded.file_name().unwrap_or_default()))
                        .unwrap_or_else(|_| expanded.clone())
                } else {
                    expanded.clone()
                }
            });

            if !expanded_canonical.starts_with(&workspace) {
                return Err(format!(
                    "Path '{}' is outside the workspace directory '{}'",
                    expanded.display(),
                    workspace.display()
                ));
            }
        }

        // Check forbidden paths using Path::starts_with() instead of String::contains()
        let home_dir = std::env::var("HOME").unwrap_or_default();
        for forbidden in &self.forbidden_paths {
            let forbidden_path = if forbidden.starts_with("~") {
                PathBuf::from(&home_dir).join(
                    forbidden
                        .strip_prefix("~/")
                        .unwrap_or(forbidden.strip_prefix("~").unwrap_or(forbidden)),
                )
            } else {
                PathBuf::from(forbidden)
            };
            if expanded.starts_with(&forbidden_path) {
                return Err(format!(
                    "Path '{}' is in the forbidden list",
                    expanded.display()
                ));
            }
        }

        Ok(())
    }

    /// Assess the risk level of a command.
    pub fn assess_command_risk(&self, command: &str) -> CommandRiskLevel {
        let cmd_lower = command.to_lowercase();

        // High risk commands
        let high_risk = [
            "rm -rf",
            "rm -r /",
            "mkfs",
            "dd",
            "> /dev/sd",
            "curl | sh",
            "wget | sh",
            "chmod 777",
            "kill -9",
            "pkill -9",
            "reboot",
            "shutdown",
        ];
        if high_risk.iter().any(|c| cmd_lower.contains(c)) {
            return CommandRiskLevel::High;
        }

        // Medium risk commands
        let medium_risk = [
            "rm -r",
            "rm -f",
            "chmod",
            "chown",
            "sudo",
            "su",
            "apt",
            "yum",
            "dnf",
            "pip install",
            "cargo install",
            "npm install -g",
            "gem install",
            "curl",
            "wget",
        ];
        if medium_risk.iter().any(|c| cmd_lower.contains(c)) {
            return CommandRiskLevel::Medium;
        }

        CommandRiskLevel::Low
    }

    /// Check if a command requires approval based on risk level.
    pub fn requires_approval(&self, command: &str) -> bool {
        let risk = self.assess_command_risk(command);
        self.autonomy.requires_approval(risk)
    }

    /// Record an action and check rate limits.
    pub fn record_action(&self) -> Result<(), String> {
        let count = self.tracker.record();
        if count > self.max_actions_per_hour as usize {
            return Err(format!(
                "Rate limit exceeded: {} actions this hour (max: {})",
                count, self.max_actions_per_hour
            ));
        }
        Ok(())
    }

    /// Validate a complete command execution request.
    pub fn validate_execution(
        &self,
        command: &str,
        path: Option<&Path>,
    ) -> Result<CommandRiskLevel, String> {
        // Check command
        self.check_command(command)?;

        // Check path if provided
        if let Some(p) = path {
            self.check_path(p)?;
        }

        // Check rate limit
        self.record_action()?;

        // Return risk level
        Ok(self.assess_command_risk(command))
    }
}

impl octo_types::PathValidator for SecurityPolicy {
    fn check_path(&self, path: &Path) -> Result<(), String> {
        SecurityPolicy::check_path(self, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = SecurityPolicy::default();
        assert_eq!(policy.autonomy, AutonomyLevel::Supervised);
        assert!(!policy.allowed_commands.is_empty());
    }

    #[test]
    fn test_command_whitelist() {
        let policy = SecurityPolicy::default();

        // Allowed command
        assert!(policy.check_command("ls").is_ok());
        assert!(policy.check_command("git status").is_ok());

        // Blocked command
        assert!(policy.check_command("rm -rf /").is_err());
    }

    #[test]
    fn test_path_forbidden() {
        let policy = SecurityPolicy::default();

        // Forbidden paths
        assert!(policy.check_path(Path::new("/etc/passwd")).is_err());
        assert!(policy.check_path(Path::new("/root/.ssh")).is_err());
    }

    #[test]
    fn test_assess_risk() {
        let policy = SecurityPolicy::default();

        assert_eq!(policy.assess_command_risk("ls"), CommandRiskLevel::Low);
        assert_eq!(policy.assess_command_risk("rm -rf"), CommandRiskLevel::High);
        assert_eq!(
            policy.assess_command_risk("chmod 777"),
            CommandRiskLevel::High
        );
        assert_eq!(policy.assess_command_risk("curl"), CommandRiskLevel::Medium);
    }

    #[test]
    fn test_autonomy_levels() {
        let read_only = SecurityPolicy::new().with_autonomy(AutonomyLevel::ReadOnly);
        assert!(!read_only.autonomy.allows_execution());
        assert!(read_only.check_command("ls").is_err());

        let full = SecurityPolicy::new().with_autonomy(AutonomyLevel::Full);
        assert!(full.autonomy.allows_execution());
    }
}
