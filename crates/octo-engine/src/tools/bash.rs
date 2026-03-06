use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

// Sandbox imports - feature-gated
#[cfg(feature = "sandbox-wasm")]
use crate::sandbox::{AdapterEnum, SandboxRouter, SandboxType, SubprocessAdapter, ToolCategory};

/// Shell 命令执行安全模式（参考 ARCHITECTURE_DESIGN.md §5.5.1）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecSecurityMode {
    /// 禁止所有 shell 执行
    Deny,
    /// 仅允许白名单命令（默认）
    Allowlist,
    /// 允许所有命令（开发模式）
    Full,
}

/// 工具执行安全策略
#[derive(Debug, Clone)]
pub struct ExecPolicy {
    pub mode: ExecSecurityMode,
    /// 内置安全命令集
    pub safe_bins: Vec<String>,
    /// 用户扩展白名单
    pub allowed_commands: Vec<String>,
}

impl Default for ExecPolicy {
    fn default() -> Self {
        Self {
            mode: ExecSecurityMode::Allowlist,
            safe_bins: vec![
                "ls", "cat", "head", "tail", "grep", "find", "echo", "pwd", "wc", "sort", "uniq",
                "cut", "awk", "sed", "tr", "diff", "git", "cargo", "npm", "python3", "python",
                "node", "touch", "mkdir",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            allowed_commands: vec![],
        }
    }
}

impl ExecPolicy {
    /// 检查命令是否被允许执行
    pub fn is_allowed(&self, command: &str) -> bool {
        match self.mode {
            ExecSecurityMode::Deny => false,
            ExecSecurityMode::Full => true,
            ExecSecurityMode::Allowlist => {
                // Block shell metacharacters that could bypass the allowlist
                if command.contains(';')
                    || command.contains('|')
                    || command.contains("&&")
                    || command.contains("||")
                    || command.contains("$(")
                    || command.contains('`')
                    || command.contains('>')
                    || command.contains('<')
                    || command.contains('\n')
                    || command.contains('\0')
                {
                    return false;
                }
                // 提取命令名（取第一个词，去掉路径前缀）
                let cmd = command.split_whitespace().next().unwrap_or("");
                let cmd_name = cmd.rsplit('/').next().unwrap_or(cmd);
                self.safe_bins.iter().any(|b| b == cmd_name)
                    || self.allowed_commands.iter().any(|b| b == cmd_name)
            }
        }
    }
}

/// 安全环境变量白名单
const SAFE_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "TERM",
    "USER",
    "SHELL",
    "CARGO_HOME",
    "RUSTUP_HOME",
];

pub struct BashTool {
    exec_policy: Option<ExecPolicy>,
    /// Sandbox router for secure execution (feature-gated)
    #[cfg(feature = "sandbox-wasm")]
    router: Option<SandboxRouter>,
}

impl BashTool {
    pub fn new() -> Self {
        #[cfg(feature = "sandbox-wasm")]
        let router = Some(SandboxRouter::new());
        Self {
            exec_policy: Some(ExecPolicy::default()),
            #[cfg(feature = "sandbox-wasm")]
            router,
        }
    }

    /// 创建带安全策略的 BashTool
    pub fn with_policy(policy: ExecPolicy) -> Self {
        #[cfg(feature = "sandbox-wasm")]
        let router = Some(SandboxRouter::new());
        Self {
            exec_policy: Some(policy),
            #[cfg(feature = "sandbox-wasm")]
            router,
        }
    }

    /// 执行命令 - 优先使用沙箱，失败则回退到直接执行
    #[cfg(feature = "sandbox-wasm")]
    async fn execute_via_sandbox(
        &self,
        command: &str,
        working_dir: &std::path::Path,
    ) -> Result<(String, i32), String> {
        use crate::sandbox::ExecResult;

        if let Some(router) = &self.router {
            // Clone the router to allow mutation
            let mut router = router.clone();
            // 注册 subprocess 适配器
            router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));
            // 使用 subprocess 作为默认执行器
            router.set_mapping(ToolCategory::Shell, SandboxType::Subprocess);

            // 在指定工作目录中执行
            let full_command = format!("cd {} && {}", working_dir.display(), command);

            match router
                .execute(ToolCategory::Shell, &full_command, "bash")
                .await
            {
                Ok(ExecResult {
                    stdout,
                    stderr,
                    success,
                    exit_code,
                    ..
                }) => {
                    let combined = if stderr.is_empty() {
                        stdout
                    } else if stdout.is_empty() {
                        format!("STDERR:\n{stderr}")
                    } else {
                        format!("{stdout}\nSTDERR:\n{stderr}")
                    };
                    let code = if success { 0 } else { exit_code };
                    return Ok((combined, code));
                }
                Err(e) => {
                    // 沙箱执行失败，回退到直接执行
                    tracing::warn!(
                        "Sandbox execution failed, falling back to direct execution: {}",
                        e
                    );
                }
            }
        }
        Err("Sandbox not available".to_string())
    }

    /// 克隆路由器（用于测试）
    #[cfg(feature = "sandbox-wasm")]
    pub fn router(&self) -> Option<&SandboxRouter> {
        self.router.as_ref()
    }

    /// 设置自定义沙箱路由器（用于测试或高级配置）
    #[cfg(feature = "sandbox-wasm")]
    pub fn set_router(&mut self, router: SandboxRouter) {
        self.router = Some(router);
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
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

        // 1. 路径遍历检查
        if command.contains("../") || command.contains("..\\") {
            return Ok(ToolResult::error(
                "Security violation: path traversal detected in command".to_string(),
            ));
        }

        // 2. ExecPolicy 模式检查
        if let Some(policy) = &self.exec_policy {
            if !policy.is_allowed(command) {
                return Ok(ToolResult::error(format!(
                    "Security violation: command not allowed by exec policy (mode={:?})",
                    policy.mode
                )));
            }
        }

        let timeout_secs = params["timeout"].as_u64().unwrap_or(30).min(120);

        debug!(command, timeout_secs, "executing bash command");

        // 尝试沙箱执行（如果启用）
        #[cfg(feature = "sandbox-wasm")]
        {
            match self.execute_via_sandbox(command, &ctx.working_dir).await {
                Ok((output_text, exit_code)) => {
                    // 截断过长输出
                    let output_text = if output_text.len() > 100_000 {
                        format!(
                            "{}...\n[output truncated, {} bytes total]",
                            &output_text[..100_000],
                            output_text.len()
                        )
                    } else {
                        output_text
                    };

                    if exit_code == 0 {
                        return Ok(ToolResult::success(output_text));
                    } else {
                        return Ok(ToolResult::error(format!(
                            "Exit code: {exit_code}\n{output_text}"
                        )));
                    }
                }
                Err(_) => {
                    // 沙箱不可用或失败，继续使用直接执行
                    tracing::warn!("Sandbox not available, falling back to direct command execution");
                }
            }
        }

        // 直接执行（默认行为或沙箱回退）
        // 收集安全环境变量白名单
        let safe_env: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| SAFE_ENV_VARS.contains(&k.as_str()))
            .collect();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("bash")
                .arg("-c")
                .arg(command)
                .current_dir(&ctx.working_dir)
                .env_clear()
                .envs(safe_env)
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
