//! eaasp-scoped-hook-mcp — stdio MCP proxy with ADR-V2-006 hook dispatch.
//!
//! Intercepts `tools/call` JSON-RPC requests, dispatches PreToolUse/PostToolUse
//! bash hooks (exit-code contract: exit 2 = deny, 0 = allow, else fail-open),
//! and forwards all other messages transparently to the downstream MCP process.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ── ADR-V2-006 envelope types ────────────────────────────────

/// Envelope written to hook script stdin (ADR-V2-006 §2).
#[derive(Debug, Clone, Serialize)]
pub struct HookEnvelope {
    pub hook_id: String,
    pub event: String,
    pub session_id: String,
    pub skill_id: String,
    pub tool_name: String,
    pub input_json: String,
}

/// Decision returned from hook script (ADR-V2-006 §2).
#[derive(Debug, Deserialize)]
pub struct HookResult {
    pub decision: String, // "allow" | "deny"
    #[serde(default)]
    pub reason: String,
}

/// Dispatch a hook script with the given envelope.
///
/// Exit-code contract (ADR-V2-006 §3):
///   - exit 0  → allow
///   - exit 2  → deny (reason from stdout JSON)
///   - any error / timeout → fail-open (allow)
pub async fn dispatch_hook(
    script_path: &Path,
    envelope: &HookEnvelope,
    timeout_secs: u64,
) -> HookResult {
    let envelope_json = match serde_json::to_string(envelope) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(error = %e, "dispatch_hook: failed to serialize envelope — fail-open");
            return HookResult { decision: "allow".to_string(), reason: String::new() };
        }
    };

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        run_hook_script(script_path, &envelope_json),
    )
    .await;

    match result {
        Ok(Ok((exit_code, stdout))) => {
            if exit_code == 2 {
                // Parse reason from stdout JSON if present.
                let reason = serde_json::from_str::<HookResult>(&stdout)
                    .map(|r| r.reason)
                    .unwrap_or_default();
                tracing::debug!(hook = %script_path.display(), reason = %reason, "hook denied");
                HookResult { decision: "deny".to_string(), reason }
            } else {
                HookResult { decision: "allow".to_string(), reason: String::new() }
            }
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, hook = %script_path.display(), "hook error — fail-open");
            HookResult { decision: "allow".to_string(), reason: String::new() }
        }
        Err(_) => {
            tracing::warn!(hook = %script_path.display(), timeout = timeout_secs, "hook timeout — fail-open");
            HookResult { decision: "allow".to_string(), reason: String::new() }
        }
    }
}

async fn run_hook_script(script_path: &Path, stdin_data: &str) -> Result<(i32, String)> {
    let mut child = Command::new("sh")
        .arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn hook script: {}", script_path.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data.as_bytes())
            .await
            .context("failed to write envelope to hook stdin")?;
        // drop stdin to signal EOF
    }

    let output = child.wait_with_output().await.context("hook wait_with_output failed")?;
    let exit_code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((exit_code, stdout))
}

// ── Downstream MCP process ───────────────────────────────────

/// Spawns and manages a downstream MCP process (stdio transport).
pub struct DownstreamMcp {
    child: Mutex<Child>,
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
}

impl DownstreamMcp {
    pub fn spawn(cmd: &str, args: &[String]) -> Result<Self> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to spawn downstream MCP: {cmd}"))?;

        let stdin = child.stdin.take().context("downstream stdin unavailable")?;
        let stdout = child.stdout.take().context("downstream stdout unavailable")?;

        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
        })
    }

    /// Send a line to downstream and read one line back.
    pub async fn roundtrip(&self, request_json: &str) -> Result<String> {
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(format!("{request_json}\n").as_bytes())
                .await
                .context("write to downstream stdin")?;
            stdin.flush().await.context("flush downstream stdin")?;
        }
        let mut line = String::new();
        self.stdout
            .lock()
            .await
            .read_line(&mut line)
            .await
            .context("read from downstream stdout")?;
        Ok(line)
    }
}

// ── ProxyServer ──────────────────────────────────────────────

/// MCP stdio proxy that intercepts `tools/call` for hook dispatch.
pub struct ProxyServer {
    downstream: Arc<DownstreamMcp>,
    hook_dir: Option<PathBuf>,
    session_id: String,
    skill_id: String,
    hook_timeout_secs: u64,
}

impl ProxyServer {
    pub fn new(
        downstream: Arc<DownstreamMcp>,
        hook_dir: Option<PathBuf>,
        session_id: impl Into<String>,
        skill_id: impl Into<String>,
        hook_timeout_secs: u64,
    ) -> Self {
        Self {
            downstream,
            hook_dir,
            session_id: session_id.into(),
            skill_id: skill_id.into(),
            hook_timeout_secs,
        }
    }

    /// Process one raw JSON-RPC line. Returns the response line to write to stdout,
    /// or None if the message should be dropped (hook denied and no response needed).
    pub async fn handle_message(&self, raw: &str) -> Option<String> {
        // Parse loosely — only need "method" and "params.name".
        let parsed: serde_json::Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => return Some(raw.to_string()),
        };

        let method = parsed["method"].as_str().unwrap_or("");

        if method == "tools/call" {
            let tool_name = parsed["params"]["name"].as_str().unwrap_or("").to_string();
            let input_json = serde_json::to_string(&parsed["params"]["arguments"])
                .unwrap_or_default();

            // PreToolUse hook
            if let Some(pre_script) = find_hook_script(self.hook_dir.as_deref(), "pre_tool_use") {
                let envelope = HookEnvelope {
                    hook_id: format!("pre_tool_use_{tool_name}"),
                    event: "PRE_TOOL_USE".to_string(),
                    session_id: self.session_id.clone(),
                    skill_id: self.skill_id.clone(),
                    tool_name: tool_name.clone(),
                    input_json: input_json.clone(),
                };
                let result = dispatch_hook(&pre_script, &envelope, self.hook_timeout_secs).await;
                if result.decision == "deny" {
                    // Return a JSON-RPC error response.
                    let id = &parsed["id"];
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32600,
                            "message": format!("hook denied: {}", result.reason)
                        }
                    });
                    return Some(resp.to_string());
                }
            }

            // Forward to downstream.
            let downstream_resp = match self.downstream.roundtrip(raw).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "downstream roundtrip failed");
                    return Some(raw.to_string());
                }
            };

            // PostToolUse hook — fire-and-forget, never blocks response.
            if let Some(post_script) = find_hook_script(self.hook_dir.as_deref(), "post_tool_use") {
                let envelope = HookEnvelope {
                    hook_id: format!("post_tool_use_{tool_name}"),
                    event: "POST_TOOL_USE".to_string(),
                    session_id: self.session_id.clone(),
                    skill_id: self.skill_id.clone(),
                    tool_name: tool_name.clone(),
                    input_json: input_json.clone(),
                };
                let timeout = self.hook_timeout_secs;
                tokio::spawn(async move {
                    dispatch_hook(&post_script, &envelope, timeout).await;
                });
            }

            Some(downstream_resp)
        } else {
            // Transparent proxy for all other messages.
            match self.downstream.roundtrip(raw).await {
                Ok(r) => Some(r),
                Err(e) => {
                    tracing::error!(error = %e, method = %method, "downstream proxy failed");
                    None
                }
            }
        }
    }
}

/// Look for `{hook_dir}/{hook_type}.sh`. Returns None if hook_dir is None or file absent.
pub fn find_hook_script(hook_dir: Option<&Path>, hook_type: &str) -> Option<PathBuf> {
    let dir = hook_dir?;
    let path = dir.join(format!("{hook_type}.sh"));
    if path.exists() { Some(path) } else { None }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn make_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[tokio::test]
    async fn test_dispatch_hook_allow_on_exit_0() {
        let dir = tempfile::tempdir().unwrap();
        let script = make_script(dir.path(), "allow.sh", "#!/bin/sh\nexit 0\n");
        let envelope = HookEnvelope {
            hook_id: "h1".into(),
            event: "PRE_TOOL_USE".into(),
            session_id: "s1".into(),
            skill_id: "sk1".into(),
            tool_name: "bash".into(),
            input_json: "{}".into(),
        };
        let result = dispatch_hook(&script, &envelope, 5).await;
        assert_eq!(result.decision, "allow");
    }

    #[tokio::test]
    async fn test_dispatch_hook_deny_on_exit_2() {
        let dir = tempfile::tempdir().unwrap();
        let script = make_script(
            dir.path(),
            "deny.sh",
            "#!/bin/sh\necho '{\"decision\":\"deny\",\"reason\":\"blocked\"}'\nexit 2\n",
        );
        let envelope = HookEnvelope {
            hook_id: "h2".into(),
            event: "PRE_TOOL_USE".into(),
            session_id: "s1".into(),
            skill_id: "sk1".into(),
            tool_name: "bash".into(),
            input_json: "{}".into(),
        };
        let result = dispatch_hook(&script, &envelope, 5).await;
        assert_eq!(result.decision, "deny");
        assert_eq!(result.reason, "blocked");
    }

    #[tokio::test]
    async fn test_dispatch_hook_fail_open_on_timeout() {
        let dir = tempfile::tempdir().unwrap();
        // sleep 10 will exceed 1s timeout
        let script = make_script(dir.path(), "slow.sh", "#!/bin/sh\nsleep 10\nexit 2\n");
        let envelope = HookEnvelope {
            hook_id: "h3".into(),
            event: "PRE_TOOL_USE".into(),
            session_id: "s1".into(),
            skill_id: "sk1".into(),
            tool_name: "bash".into(),
            input_json: "{}".into(),
        };
        let result = dispatch_hook(&script, &envelope, 1).await;
        // Fail-open: must be allow despite exit 2 intent
        assert_eq!(result.decision, "allow");
    }

    #[tokio::test]
    async fn test_find_hook_script_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        // hook_dir exists but no pre_tool_use.sh
        let result = find_hook_script(Some(dir.path()), "pre_tool_use");
        assert!(result.is_none());

        // hook_dir is None
        let result2 = find_hook_script(None, "pre_tool_use");
        assert!(result2.is_none());
    }
}
