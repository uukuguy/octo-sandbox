use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use super::traits::{RuntimeType, SkillRuntime};
use super::SkillContext;

/// Node.js runtime for executing JavaScript skills.
pub struct NodeJsRuntime {
    /// Path to the node binary (auto-detected or explicit).
    node_path: PathBuf,
    /// Execution timeout.
    timeout: Duration,
}

impl Default for NodeJsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeJsRuntime {
    /// Create a new Node.js runtime with auto-detected node binary.
    pub fn new() -> Self {
        let node_path = Self::find_node_path();
        Self {
            node_path,
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the execution timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set an explicit path to the node binary.
    pub fn with_node_path(mut self, path: PathBuf) -> Self {
        self.node_path = path;
        self
    }

    /// Find the node binary path using `which` command on Unix or `where` on Windows.
    fn find_node_path() -> PathBuf {
        #[cfg(windows)]
        let cmd = "where";
        #[cfg(not(windows))]
        let cmd = "which";

        std::process::Command::new(cmd)
            .arg("node")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let path = stdout.lines().next()?.trim().to_string();
                if path.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(path))
                }
            })
            .unwrap_or_else(|| PathBuf::from("node"))
    }
}

#[async_trait]
impl SkillRuntime for NodeJsRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::NodeJS
    }

    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value> {
        let args_json = serde_json::to_string(&args)?;

        // Use profile-aware timeout if available, else fall back to configured default
        let effective_timeout = if context.sandbox_profile.is_some() {
            std::time::Duration::from_secs(context.effective_timeout_secs())
        } else {
            self.timeout
        };

        let output = tokio::time::timeout(effective_timeout, {
            Command::new(&self.node_path)
                .arg("-e")
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
        .context("Node.js execution timed out")?
        .context("Failed to execute Node.js script")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Node.js script failed: {}", stderr));
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
        let output = Command::new(&self.node_path)
            .arg("--version")
            .output()
            .await
            .context("Node.js not found")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Node.js check failed"));
        }

        let version = String::from_utf8_lossy(&output.stdout);
        debug!("Node.js version: {}", version.trim());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn node_available() -> bool {
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_nodejs_runtime_type() {
        let runtime = NodeJsRuntime::new();
        assert_eq!(runtime.runtime_type(), RuntimeType::NodeJS);
    }

    #[tokio::test]
    async fn test_nodejs_execute_simple() {
        if !node_available() {
            println!("Node.js not installed, skipping test");
            return;
        }

        let runtime = NodeJsRuntime::new();
        let context = SkillContext::new("test_node".to_string(), PathBuf::from("/tmp"));
        let args = serde_json::json!({});

        let result = runtime
            .execute(
                r#"console.log(JSON.stringify({result: "ok"}))"#,
                args,
                &context,
            )
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["result"], "ok");
    }

    #[tokio::test]
    async fn test_nodejs_execute_with_args() {
        if !node_available() {
            println!("Node.js not installed, skipping test");
            return;
        }

        let runtime = NodeJsRuntime::new();
        let context = SkillContext::new("test_node_args".to_string(), PathBuf::from("/tmp"));
        let args = serde_json::json!({"name": "world"});

        let script = r#"
const args = JSON.parse(process.env.SKILL_ARGS || '{}');
console.log(JSON.stringify({greeting: `hello ${args.name}`}));
"#;

        let result = runtime.execute(script, args, &context).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["greeting"], "hello world");
    }

    #[tokio::test]
    async fn test_nodejs_execute_error() {
        if !node_available() {
            println!("Node.js not installed, skipping test");
            return;
        }

        let runtime = NodeJsRuntime::new();
        let context = SkillContext::new("test_node_err".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("throw new Error('boom');", serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_nodejs_execute_plain_text() {
        if !node_available() {
            println!("Node.js not installed, skipping test");
            return;
        }

        let runtime = NodeJsRuntime::new();
        let context = SkillContext::new("test_node_text".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute(
                "console.log('just a string');",
                serde_json::json!({}),
                &context,
            )
            .await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val, serde_json::Value::String("just a string".to_string()));
    }

    #[tokio::test]
    async fn test_nodejs_check_environment() {
        if !node_available() {
            println!("Node.js not installed, skipping test");
            return;
        }

        let runtime = NodeJsRuntime::new();
        let result = runtime.check_environment().await;
        assert!(result.is_ok());
    }
}
