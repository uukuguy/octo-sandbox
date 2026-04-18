//! ClawCodeAdapter — subprocess lifecycle for claw-code UltraWorkers.
//!
//! Spawns `claw-code` (or the binary pointed to by CLAW_CODE_BIN) as a
//! subprocess and communicates via stdin/stdout JSON-RPC similar to the
//! goose ACP adapter. Actual wiring is stub-only; real UltraWorkers
//! protocol integration lands in a follow-up task.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::ultra_worker::UltraWorkerEvent;

/// Per-session handle to a running claw-code subprocess.
struct SessionHandle {
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    _child: Child,
}

/// Multi-session adapter that manages claw-code subprocess lifecycle.
pub struct ClawCodeAdapter {
    deployment_mode: String,
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
}

#[derive(Default)]
pub struct SessionConfig {
    pub model: Option<String>,
}

impl ClawCodeAdapter {
    pub fn new() -> Self {
        Self::with_mode("shared")
    }

    pub fn with_mode(mode: impl Into<String>) -> Self {
        Self {
            deployment_mode: mode.into(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn deployment_mode(&self) -> &str {
        &self.deployment_mode
    }

    /// Spawn a new claw-code subprocess session. Returns the session ID.
    pub async fn start_session(&self, _config: SessionConfig) -> Result<String> {
        let bin = std::env::var("CLAW_CODE_BIN").unwrap_or_else(|_| "claw-code".to_string());
        let which_result = which::which(&bin);

        if which_result.is_err() {
            // Gate: claw-code binary not installed — return stub session.
            let sid = Uuid::new_v4().to_string();
            tracing::warn!(
                sid = %sid,
                bin = %bin,
                "claw-code binary not found; using stub session (set CLAW_CODE_BIN to enable)"
            );
            return Ok(sid);
        }

        let mut child = Command::new(&bin)
            .args(["--ultra", "--stdio"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(BufReader::new);

        let sid = Uuid::new_v4().to_string();
        self.sessions.lock().await.insert(
            sid.clone(),
            SessionHandle { stdin, stdout, _child: child },
        );
        Ok(sid)
    }

    /// Send a message to a session via stdin.
    pub async fn send_message(&self, sid: &str, content: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let handle = sessions
            .get_mut(sid)
            .ok_or_else(|| anyhow!("session {sid} not found"))?;

        if let Some(stdin) = &mut handle.stdin {
            let msg = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "session/send",
                "params": { "session_id": sid, "content": content }
            });
            let line = format!("{}\n", msg);
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    /// Read the next UltraWorker event from a session's stdout.
    /// Returns None on EOF.
    pub async fn next_event(&self, sid: &str) -> Result<Option<UltraWorkerEvent>> {
        let mut sessions = self.sessions.lock().await;
        let handle = sessions
            .get_mut(sid)
            .ok_or_else(|| anyhow!("session {sid} not found"))?;

        if let Some(stdout) = &mut handle.stdout {
            let mut line = String::new();
            let n = stdout.read_line(&mut line).await?;
            if n == 0 {
                return Ok(None);
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                return Ok(Some(UltraWorkerEvent::Unknown { raw: String::new() }));
            }
            let ev = UltraWorkerEvent::try_from(trimmed)
                .unwrap_or_else(|_| UltraWorkerEvent::Unknown { raw: trimmed.to_string() });
            return Ok(Some(ev));
        }
        Ok(None)
    }

    /// Terminate a session subprocess.
    pub async fn stop_session(&self, sid: &str) -> Result<()> {
        self.sessions.lock().await.remove(sid);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_session_returns_id_when_binary_absent() {
        // CLAW_CODE_BIN points to non-existent binary → stub path
        std::env::set_var("CLAW_CODE_BIN", "/tmp/__nonexistent_claw_code__");
        let adapter = ClawCodeAdapter::new();
        let sid = adapter.start_session(SessionConfig::default()).await.unwrap();
        assert!(!sid.is_empty());
    }

    #[tokio::test]
    async fn send_message_errors_on_unknown_session() {
        let adapter = ClawCodeAdapter::new();
        let result = adapter.send_message("no-such-session", "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stop_session_is_idempotent() {
        std::env::set_var("CLAW_CODE_BIN", "/tmp/__nonexistent_claw_code__");
        let adapter = ClawCodeAdapter::new();
        let sid = adapter.start_session(SessionConfig::default()).await.unwrap();
        adapter.stop_session(&sid).await.unwrap();
        adapter.stop_session(&sid).await.unwrap(); // second call should not error
    }
}
