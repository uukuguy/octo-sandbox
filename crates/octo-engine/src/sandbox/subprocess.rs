//! Subprocess sandbox adapter for local code execution

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;

/// Local subprocess sandbox adapter
/// Executes commands in isolated temporary directories
#[derive(Clone)]
pub struct SubprocessAdapter {
    instances: Arc<RwLock<HashMap<SandboxId, SubprocessInstance>>>,
}

/// Internal representation of a subprocess sandbox instance
struct SubprocessInstance {
    config: SandboxConfig,
    working_dir: std::path::PathBuf,
}

impl SubprocessAdapter {
    /// Create a new SubprocessAdapter
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for SubprocessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for SubprocessAdapter {
    /// Returns the sandbox type
    fn sandbox_type(&self) -> SandboxType {
        SandboxType::Subprocess
    }

    /// Create a new sandbox instance with isolated working directory
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        let id = SandboxId::new(uuid::Uuid::new_v4().to_string());

        // Determine working directory
        let working_dir = if let Some(ref dir) = config.working_dir {
            dir.clone()
        } else {
            std::env::temp_dir()
                .join("octo-sandbox")
                .join(id.to_string())
        };

        // Create working directory
        std::fs::create_dir_all(&working_dir).map_err(SandboxError::IoError)?;

        let instance = SubprocessInstance {
            config: config.clone(),
            working_dir,
        };

        let mut instances = self.instances.write().await;
        instances.insert(id.clone(), instance);

        tracing::debug!("Created subprocess sandbox: {}", id);
        Ok(id)
    }

    /// Execute a command in the sandbox
    async fn execute(
        &self,
        id: &SandboxId,
        code: &str,
        _language: &str,
    ) -> Result<ExecResult, SandboxError> {
        let instances = self.instances.read().await;
        let instance = instances
            .get(id)
            .ok_or_else(|| SandboxError::NotFound(id.clone()))?;

        let start = std::time::Instant::now();

        // Execute command using shell
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(code)
            .current_dir(&instance.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();

        // Set allowed environment variables from config
        for (key, value) in &instance.config.env {
            cmd.env(key, value);
        }

        // Set default PATH if not specified
        if !instance.config.env.contains_key("PATH") {
            cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| SandboxError::ExecutionFailed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        tracing::debug!(
            "Executed command in sandbox {}: exit_code={}, duration_ms={}",
            id,
            output.status.code().unwrap_or(-1),
            duration_ms
        );

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            execution_time_ms: duration_ms,
            success: output.status.success(),
        })
    }

    /// Destroy a sandbox instance and clean up its working directory
    async fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        let mut instances = self.instances.write().await;

        if let Some(instance) = instances.remove(id) {
            // Clean up working directory
            if instance.working_dir.exists() {
                std::fs::remove_dir_all(&instance.working_dir).map_err(SandboxError::IoError)?;
            }
            tracing::debug!("Destroyed subprocess sandbox: {}", id);
        }

        Ok(())
    }
}
