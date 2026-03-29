//! Command executor for declarative hooks.
//!
//! Runs external scripts via `sh -c`, passing HookContext through environment
//! variables (fast path) and stdin JSON (full data). Parses the stdout JSON
//! response to determine the hook decision.

use serde::{Deserialize, Serialize};

use crate::hooks::HookContext;

/// Decision returned by a command-type hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDecision {
    /// "allow", "deny", or "ask"
    pub decision: String,
    /// Explanation for the decision.
    #[serde(default)]
    pub reason: Option<String>,
    /// Modified tool input (Mutating hook).
    #[serde(rename = "updatedInput", default)]
    pub updated_input: Option<serde_json::Value>,
    /// Message to inject into agent context.
    #[serde(rename = "systemMessage", default)]
    pub system_message: Option<String>,
}

impl HookDecision {
    pub fn is_allow(&self) -> bool {
        self.decision == "allow"
    }

    pub fn is_deny(&self) -> bool {
        self.decision == "deny"
    }

    pub fn is_ask(&self) -> bool {
        self.decision == "ask"
    }
}

/// Execute an external command as a hook, passing context via env + stdin.
///
/// Protocol:
/// - Environment variables: OCTO_* prefix (from `ctx.to_env_vars()`)
/// - Stdin: Full context JSON (from `ctx.to_json()`)
/// - Stdout: JSON `HookDecision` (exit 0)
/// - Exit code 2: Error feedback (stderr as reason)
/// - Other non-zero: Handled by failure_mode
pub async fn execute_command(
    command: &str,
    ctx: &HookContext,
    timeout_secs: u32,
) -> anyhow::Result<HookDecision> {
    use tokio::process::Command;

    let env_vars = ctx.to_env_vars();
    let stdin_json = serde_json::to_string(&ctx.to_json())?;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .envs(env_vars)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Write context JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        // Best-effort write; don't fail if the script doesn't read stdin
        let _ = stdin.write_all(stdin_json.as_bytes()).await;
        drop(stdin);
    }

    // Wait with timeout
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs as u64),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Command timed out after {}s: {}", timeout_secs, command))??;

    parse_command_output(&output)
}

/// Parse command output into a HookDecision.
fn parse_command_output(output: &std::process::Output) -> anyhow::Result<HookDecision> {
    if output.status.success() {
        // Exit 0: parse stdout as JSON
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            // No output = allow
            return Ok(HookDecision {
                decision: "allow".into(),
                reason: None,
                updated_input: None,
                system_message: None,
            });
        }
        serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse hook command stdout as JSON: {e}\nOutput: {}",
                &trimmed[..trimmed.len().min(200)]
            )
        })
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if code == 2 {
            // Exit 2: structured error feedback
            Ok(HookDecision {
                decision: "deny".into(),
                reason: Some(stderr.trim().to_string()),
                updated_input: None,
                system_message: None,
            })
        } else {
            // Other non-zero: propagate as error (handled by failure_mode)
            Err(anyhow::anyhow!(
                "Hook command exited with code {}: {}",
                code,
                stderr.trim()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn test_parse_allow_output() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: br#"{"decision": "allow", "reason": "looks safe"}"#.to_vec(),
            stderr: vec![],
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_allow());
        assert_eq!(decision.reason.as_deref(), Some("looks safe"));
    }

    #[test]
    fn test_parse_deny_output() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: br#"{"decision": "deny", "reason": "dangerous path"}"#.to_vec(),
            stderr: vec![],
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_deny());
    }

    #[test]
    fn test_parse_ask_output() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: br#"{"decision": "ask", "reason": "requires human review"}"#.to_vec(),
            stderr: vec![],
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_ask());
    }

    #[test]
    fn test_parse_empty_stdout_allows() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_allow());
    }

    #[test]
    fn test_parse_exit_2_denies() {
        // Exit code 2 in raw status: code << 8 = 2 << 8 = 512
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(2 << 8),
            stdout: vec![],
            stderr: b"Path blocked by policy".to_vec(),
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_deny());
        assert_eq!(decision.reason.as_deref(), Some("Path blocked by policy"));
    }

    #[test]
    fn test_parse_other_exit_error() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: vec![],
            stderr: b"script crashed".to_vec(),
        };
        let result = parse_command_output(&output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exited with code 1"));
    }

    #[test]
    fn test_parse_updated_input() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: br#"{"decision": "allow", "updatedInput": {"command": "ls -la --color"}}"#.to_vec(),
            stderr: vec![],
        };
        let decision = parse_command_output(&output).unwrap();
        assert!(decision.is_allow());
        assert!(decision.updated_input.is_some());
        assert_eq!(
            decision.updated_input.unwrap()["command"],
            "ls -la --color"
        );
    }

    #[test]
    fn test_parse_invalid_json_errors() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"not json at all".to_vec(),
            stderr: vec![],
        };
        let result = parse_command_output(&output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }

    #[tokio::test]
    async fn test_execute_command_echo_allow() {
        let ctx = HookContext::new()
            .with_session("s1")
            .with_tool("bash", serde_json::json!({"command": "ls"}));
        let decision = execute_command(
            r#"echo '{"decision": "allow"}'"#,
            &ctx,
            5,
        )
        .await
        .unwrap();
        assert!(decision.is_allow());
    }

    #[tokio::test]
    async fn test_execute_command_reads_env() {
        let ctx = HookContext::new()
            .with_session("test-sess")
            .with_tool("bash", serde_json::json!({"command": "ls"}));
        let decision = execute_command(
            r#"echo "{\"decision\": \"allow\", \"reason\": \"session=$OCTO_SESSION_ID tool=$OCTO_TOOL_NAME\"}""#,
            &ctx,
            5,
        )
        .await
        .unwrap();
        assert!(decision.is_allow());
        let reason = decision.reason.unwrap();
        assert!(reason.contains("test-sess"), "reason should contain session: {}", reason);
        assert!(reason.contains("bash"), "reason should contain tool: {}", reason);
    }

    #[tokio::test]
    async fn test_execute_command_timeout() {
        let ctx = HookContext::new();
        let result = execute_command("sleep 10", &ctx, 1).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_execute_command_deny_exit2() {
        let ctx = HookContext::new();
        let decision = execute_command(
            "echo 'Blocked by policy' >&2; exit 2",
            &ctx,
            5,
        )
        .await
        .unwrap();
        assert!(decision.is_deny());
        assert_eq!(decision.reason.as_deref(), Some("Blocked by policy"));
    }
}
