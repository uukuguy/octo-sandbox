use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command. Returns stdout, stderr, and exit code."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 120)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?;

        let timeout_secs = params["timeout"]
            .as_u64()
            .unwrap_or(30)
            .min(120);

        debug!(command, timeout_secs, "executing bash command");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("bash")
                .arg("-c")
                .arg(command)
                .current_dir(&ctx.working_dir)
                .env_clear()
                .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
                .env("HOME", "/tmp")
                .env("LANG", "en_US.UTF-8")
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                let combined = if stderr.is_empty() {
                    stdout
                } else if stdout.is_empty() {
                    format!("STDERR:\n{stderr}")
                } else {
                    format!("{stdout}\nSTDERR:\n{stderr}")
                };

                // Truncate if too long
                let output_text = if combined.len() > 100_000 {
                    format!(
                        "{}...\n[output truncated, {} bytes total]",
                        &combined[..100_000],
                        combined.len()
                    )
                } else {
                    combined
                };

                if exit_code == 0 {
                    Ok(ToolResult::success(output_text))
                } else {
                    Ok(ToolResult::error(format!(
                        "Exit code: {exit_code}\n{output_text}"
                    )))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!("Failed to execute command: {e}"))),
            Err(_) => Ok(ToolResult::error(format!(
                "Command timed out after {timeout_secs} seconds"
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
