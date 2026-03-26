use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use super::traits::{RuntimeType, SkillRuntime};
use super::SkillContext;

/// Shell/Bash runtime for executing shell scripts.
pub struct ShellRuntime {
    /// Shell binary (default: "bash" on Unix, "cmd" on Windows).
    shell: String,
    /// Execution timeout.
    timeout: Duration,
}

impl Default for ShellRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellRuntime {
    /// Create a new Shell runtime with platform-appropriate defaults.
    pub fn new() -> Self {
        let shell = if cfg!(target_os = "windows") {
            "cmd".to_string()
        } else {
            "bash".to_string()
        };
        Self {
            shell,
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the execution timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set an explicit shell binary.
    pub fn with_shell(mut self, shell: String) -> Self {
        self.shell = shell;
        self
    }
}

#[async_trait]
impl SkillRuntime for ShellRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Shell
    }

    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value> {
        let args_json = serde_json::to_string(&args)?;

        let flag = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        // Use profile-aware timeout if available, else fall back to configured default
        let effective_timeout = if context.sandbox_profile.is_some() {
            Duration::from_secs(context.effective_timeout_secs())
        } else {
            self.timeout
        };

        let output = tokio::time::timeout(effective_timeout, {
            Command::new(&self.shell)
                .arg(flag)
                .arg(script)
                .current_dir(&context.working_dir)
                .stdin(std::process::Stdio::null())
                .env("SKILL_NAME", &context.skill_name)
                .env("SKILL_ARGS", &args_json)
                .env(
                    "SKILL_BASE_DIR",
                    context.working_dir.to_string_lossy().as_ref(),
                )
                .output()
        })
        .await
        .context("Shell execution timed out")?
        .context("Failed to execute shell script")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Shell script failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();

        if trimmed.is_empty() {
            return Ok(serde_json::Value::Null);
        }

        // Try to parse as JSON, fall back to string
        match serde_json::from_str(trimmed) {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::Value::String(trimmed.to_string())),
        }
    }

    async fn check_environment(&self) -> Result<()> {
        let output = Command::new(&self.shell)
            .arg("--version")
            .output()
            .await
            .context("Shell not found")?;

        let version = String::from_utf8_lossy(&output.stdout);
        debug!("Shell version: {}", version.trim());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_shell_runtime_type() {
        let runtime = ShellRuntime::new();
        assert_eq!(runtime.runtime_type(), RuntimeType::Shell);
    }

    #[tokio::test]
    async fn test_shell_execute_echo() {
        let runtime = ShellRuntime::new();
        let context = SkillContext::new("test_shell".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("echo hello", serde_json::json!({}), &context)
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val, serde_json::Value::String("hello".to_string()));
    }

    #[tokio::test]
    async fn test_shell_execute_json() {
        let runtime = ShellRuntime::new();
        let context = SkillContext::new("test_shell_json".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute(r#"echo '{"key":"val"}'"#, serde_json::json!({}), &context)
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["key"], "val");
    }

    #[tokio::test]
    async fn test_shell_execute_with_env() {
        let runtime = ShellRuntime::new();
        let context = SkillContext::new("my_skill".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("echo $SKILL_NAME", serde_json::json!({}), &context)
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val, serde_json::Value::String("my_skill".to_string()));
    }

    #[tokio::test]
    async fn test_shell_execute_failure() {
        let runtime = ShellRuntime::new();
        let context = SkillContext::new("test_shell_fail".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("exit 1", serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_check_environment() {
        let runtime = ShellRuntime::new();
        let result = runtime.check_environment().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_empty_output() {
        let runtime = ShellRuntime::new();
        let context = SkillContext::new("test_shell_empty".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("true", serde_json::json!({}), &context)
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val, serde_json::Value::Null);
    }
}
