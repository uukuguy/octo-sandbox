use anyhow::Result;
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use octo_types::{ExecResult, RuntimeType};

use super::traits::RuntimeAdapter;

pub struct NativeRuntime {
    timeout_secs: u64,
}

impl NativeRuntime {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new(30)
    }
}

#[async_trait]
impl RuntimeAdapter for NativeRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Native
    }

    async fn execute(&self, cmd: &str, working_dir: &str) -> Result<ExecResult> {
        debug!(cmd, working_dir, "NativeRuntime executing");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            Command::new("bash")
                .arg("-c")
                .arg(cmd)
                .current_dir(working_dir)
                .stdin(std::process::Stdio::null())
                .env_clear()
                .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
                .env("HOME", "/tmp")
                .env("LANG", "en_US.UTF-8")
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => Ok(ExecResult {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            }),
            Ok(Err(e)) => Ok(ExecResult {
                stdout: String::new(),
                stderr: format!("Failed to execute: {e}"),
                exit_code: -1,
            }),
            Err(_) => Ok(ExecResult {
                stdout: String::new(),
                stderr: format!("Command timed out after {} seconds", self.timeout_secs),
                exit_code: -1,
            }),
        }
    }
}
