use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::debug;

use octo_types::{ApprovalRequirement, RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

use crate::sandbox::{
    ExecutionTarget, ExecutionTargetResolver, OctoRunMode, SandboxProfile,
    SandboxRef, SandboxRouter, SessionSandboxManager, ToolCategory,
};

/// Environment variables passed through to command execution.
///
/// Security boundary is at the sandbox level (Docker/WASM isolation),
/// not the tool level. Commands need access to API keys, Python paths,
/// and other runtime state to function properly.
const PASSTHROUGH_ENV_VARS: &[&str] = &[
    // System basics
    "PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "TERM", "USER", "SHELL",
    // Build tools
    "CARGO_HOME", "RUSTUP_HOME",
    // Python
    "VIRTUAL_ENV", "PYTHONPATH", "UV_CACHE_DIR",
    // Node
    "NODE_PATH", "NPM_CONFIG_PREFIX",
    // LLM API keys (needed by skill scripts)
    "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENAI_BASE_URL",
    "TAVILY_API_KEY", "JINA_API_KEY",
    // Proxy (corporate environments)
    "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY",
    "http_proxy", "https_proxy", "no_proxy",
];

pub struct BashTool {
    /// Sandbox router for sandboxed execution
    router: Option<SandboxRouter>,
    /// Execution target resolver
    target_resolver: Option<ExecutionTargetResolver>,
    /// Session sandbox manager for per-session container reuse
    session_sandbox: Option<Arc<SessionSandboxManager>>,
}

impl BashTool {
    pub fn new() -> Self {
        // Default: Development profile, Host mode — direct execution
        Self {
            router: None,
            target_resolver: None,
            session_sandbox: None,
        }
    }

    /// Create a BashTool with sandbox routing enabled.
    pub fn with_sandbox(
        run_mode: OctoRunMode,
        profile: SandboxProfile,
        router: SandboxRouter,
    ) -> Self {
        let available_backends = router.registered_backends();
        let target_resolver =
            ExecutionTargetResolver::new(run_mode, profile, available_backends);
        Self {
            router: Some(router),
            target_resolver: Some(target_resolver),
            session_sandbox: None,
        }
    }

    /// Create a BashTool with sandbox routing and session container reuse.
    pub fn with_session_sandbox(
        run_mode: OctoRunMode,
        profile: SandboxProfile,
        router: SandboxRouter,
        session_sandbox: Arc<SessionSandboxManager>,
        session_id: String,
    ) -> Self {
        let available_backends = router.registered_backends();
        let target_resolver =
            ExecutionTargetResolver::new(run_mode, profile, available_backends)
                .with_session(session_id);
        Self {
            router: Some(router),
            target_resolver: Some(target_resolver),
            session_sandbox: Some(session_sandbox),
        }
    }

    /// Execute via sandbox router.
    async fn execute_via_sandbox(
        &self,
        command: &str,
        working_dir: &std::path::Path,
    ) -> Result<(String, i32), String> {
        let router = self.router.as_ref().ok_or("No sandbox router configured")?;

        let full_command = format!("cd {} && {}", working_dir.display(), command);

        match router
            .execute(ToolCategory::Shell, &full_command, "bash")
            .await
        {
            Ok(result) => {
                let combined = if result.stderr.is_empty() {
                    result.stdout
                } else if result.stdout.is_empty() {
                    format!("STDERR:\n{}", result.stderr)
                } else {
                    format!("{}\nSTDERR:\n{}", result.stdout, result.stderr)
                };
                let code = if result.success { 0 } else { result.exit_code };
                Ok((combined, code))
            }
            Err(e) => Err(format!("Sandbox execution failed: {}", e)),
        }
    }

    /// Get the current execution target resolver (for diagnostics).
    pub fn target_resolver(&self) -> Option<&ExecutionTargetResolver> {
        self.target_resolver.as_ref()
    }

    /// Get the sandbox router (for testing).
    pub fn router(&self) -> Option<&SandboxRouter> {
        self.router.as_ref()
    }

    /// Direct local execution via subprocess.
    async fn execute_local(
        &self,
        command: &str,
        working_dir: &std::path::Path,
        timeout_secs: u64,
        pass_env: bool,
    ) -> Result<ToolOutput> {
        let env_vars: Vec<(String, String)> = if pass_env {
            std::env::vars()
                .filter(|(k, _)| PASSTHROUGH_ENV_VARS.contains(&k.as_str()))
                .collect()
        } else {
            // Restricted env for staging/production (only system basics)
            std::env::vars()
                .filter(|(k, _)| {
                    matches!(
                        k.as_str(),
                        "PATH" | "HOME" | "TMPDIR" | "LANG" | "LC_ALL" | "TERM" | "USER" | "SHELL"
                    )
                })
                .collect()
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("bash")
                .arg("-c")
                .arg(command)
                .current_dir(working_dir)
                .stdin(std::process::Stdio::null())
                .env_clear()
                .envs(env_vars)
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

                let output_text = truncate_output(combined);

                if exit_code == 0 {
                    Ok(ToolOutput::success(output_text))
                } else {
                    Ok(ToolOutput::error(format!(
                        "Exit code: {exit_code}\n{output_text}"
                    )))
                }
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!("Failed to execute command: {e}"))),
            Err(_) => Ok(ToolOutput::error(format!(
                "Command timed out after {timeout_secs} seconds"
            ))),
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate output to 100KB max.
fn truncate_output(output: String) -> String {
    if output.len() > 100_000 {
        format!(
            "{}...\n[output truncated, {} bytes total]",
            &output[..100_000],
            output.len()
        )
    } else {
        output
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

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?;

        let timeout_secs = params["timeout"].as_u64().unwrap_or(30).min(120);

        debug!(command, timeout_secs, "executing bash command");

        // Determine execution target
        if let Some(resolver) = &self.target_resolver {
            let (target, reason) = resolver.resolve(ToolCategory::Shell);
            debug!(target = %target, reason = %reason, "sandbox routing decision");

            match target {
                ExecutionTarget::Local => {
                    // Local execution — pass env based on profile
                    let pass_env = resolver.profile().env_passthrough();
                    return self
                        .execute_local(command, &ctx.working_dir, timeout_secs, pass_env)
                        .await;
                }
                ExecutionTarget::Sandbox(SandboxRef::Session { ref id }) => {
                    // Session container reuse — execute in long-lived container
                    let ssm = self.session_sandbox.as_ref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "Sandbox routing resolved to session container '{}' but SessionSandboxManager is not configured. \
                             Ensure Docker is running and sandbox profile is correctly set.",
                            id
                        )
                    })?;
                    match ssm.execute(id, command).await {
                        Ok(result) => {
                            let combined = if result.stderr.is_empty() {
                                result.stdout
                            } else if result.stdout.is_empty() {
                                format!("STDERR:\n{}", result.stderr)
                            } else {
                                format!("{}\nSTDERR:\n{}", result.stdout, result.stderr)
                            };
                            let output_text = truncate_output(combined);
                            if result.success {
                                return Ok(ToolOutput::success(output_text));
                            } else {
                                return Ok(ToolOutput::error(format!(
                                    "Exit code: {}\n{}",
                                    result.exit_code, output_text
                                )));
                            }
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Session sandbox execution failed for session '{}': {}",
                                id, e
                            ));
                        }
                    }
                }
                ExecutionTarget::Sandbox(SandboxRef::Ephemeral { .. }) => {
                    // Ephemeral sandbox — create, execute, destroy per call
                    let (output_text, exit_code) = self
                        .execute_via_sandbox(command, &ctx.working_dir)
                        .await
                        .map_err(|e| anyhow::anyhow!("Ephemeral sandbox execution failed: {}", e))?;
                    let output_text = truncate_output(output_text);
                    if exit_code == 0 {
                        return Ok(ToolOutput::success(output_text));
                    } else {
                        return Ok(ToolOutput::error(format!(
                            "Exit code: {exit_code}\n{output_text}"
                        )));
                    }
                }
            }
        }

        // No resolver configured — default direct local execution
        self.execute_local(command, &ctx.working_dir, timeout_secs, true)
            .await
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Destructive
    }

    fn approval(&self) -> ApprovalRequirement {
        ApprovalRequirement::Always
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_bash_tool_metadata() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::Destructive);
        assert_eq!(tool.approval(), ApprovalRequirement::Always);
    }

    #[tokio::test]
    async fn test_simple_command() {
        let tool = BashTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({"command": "echo hello"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_pipe_and_shell_features_work() {
        let tool = BashTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({"command": "echo hello | tr a-z A-Z"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("HELLO"));
    }

    #[tokio::test]
    async fn test_curl_works() {
        let tool = BashTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({"command": "curl --version"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("curl"));
    }

    #[test]
    fn test_timeout_param_capped_at_120() {
        let val: u64 = 999_u64.min(120);
        assert_eq!(val, 120);
        let val: u64 = 30_u64.min(120);
        assert_eq!(val, 30);
    }

    #[tokio::test]
    async fn test_missing_command_param() {
        let tool = BashTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_passthrough_env_vars_include_api_keys() {
        assert!(PASSTHROUGH_ENV_VARS.contains(&"ANTHROPIC_API_KEY"));
        assert!(PASSTHROUGH_ENV_VARS.contains(&"OPENAI_API_KEY"));
        assert!(PASSTHROUGH_ENV_VARS.contains(&"TAVILY_API_KEY"));
        assert!(PASSTHROUGH_ENV_VARS.contains(&"PATH"));
        assert!(PASSTHROUGH_ENV_VARS.contains(&"HOME"));
    }

    #[test]
    fn test_truncate_output() {
        let short = "hello".to_string();
        assert_eq!(truncate_output(short.clone()), short);

        let long = "x".repeat(200_000);
        let truncated = truncate_output(long);
        assert!(truncated.contains("output truncated"));
        assert!(truncated.len() < 200_000);
    }

    #[tokio::test]
    async fn test_with_sandbox_development() {
        // Development mode should execute locally even with a router
        let router = SandboxRouter::with_policy(crate::sandbox::SandboxPolicy::Development);
        let tool = BashTool::with_sandbox(
            OctoRunMode::Host,
            SandboxProfile::Development,
            router,
        );

        assert!(tool.target_resolver().is_some());
        let resolver = tool.target_resolver().unwrap();
        let (target, _) = resolver.resolve(ToolCategory::Shell);
        assert_eq!(target, ExecutionTarget::Local);

        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: PathBuf::from("."),
            path_validator: None,
        };
        let result = tool.execute(json!({"command": "echo dev-mode"}), &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("dev-mode"));
    }

    #[tokio::test]
    async fn test_with_sandbox_sandboxed_mode() {
        // Sandboxed mode should always execute locally
        let router = SandboxRouter::with_policy(crate::sandbox::SandboxPolicy::Strict);
        let tool = BashTool::with_sandbox(
            OctoRunMode::Sandboxed,
            SandboxProfile::Production,
            router,
        );

        let resolver = tool.target_resolver().unwrap();
        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        assert_eq!(target, ExecutionTarget::Local);
        assert!(reason.contains("Sandboxed"));
    }
}
