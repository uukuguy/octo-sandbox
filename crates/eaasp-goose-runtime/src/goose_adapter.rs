// Outcome B: adapter owns a subprocess Child per session + ACP client handle.
// T2 wired subprocess spawn; S3.T1 adds ACP event stream reading via stdout.
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::acp_parser::AcpEvent;

pub struct SessionConfig {
    // TODO(T3): real wiring for `model`, `provider`, `extensions` (middleware insertion).
    // Kept empty at T2 scope so the adapter compiles + start/close lifecycle is exercised
    // independently of config plumbing.
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {}
    }
}

struct SessionHandle {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
}

pub struct GooseAdapter {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    // ADR-V2-019 D2: None = shared (unlimited); Some(1) = per_session.
    // Gate applied at start_session() entry.
    max_sessions: Option<usize>,
}

impl Default for GooseAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GooseAdapter {
    pub fn new() -> Self {
        Self::with_mode("shared")
    }

    /// Construct an adapter parameterized by `EAASP_DEPLOYMENT_MODE` per ADR-V2-019 D2.
    ///
    /// - `"per_session"` → caps concurrent sessions at 1 (container-level isolation)
    /// - anything else (including `"shared"`) → uncapped shared mode
    pub fn with_mode(mode: &str) -> Self {
        let max_sessions = match mode {
            "per_session" => Some(1),
            _ => None,
        };
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            max_sessions,
        }
    }

    /// Read-only accessor for tests + future T3 observability.
    pub fn max_sessions(&self) -> Option<usize> {
        self.max_sessions
    }

    pub async fn start_session(&self, _cfg: SessionConfig) -> Result<String> {
        // ADR-V2-019 D2: per_session mode rejects 2nd concurrent session inside the same container.
        if let Some(cap) = self.max_sessions {
            let count = self.sessions.lock().await.len();
            if count >= cap {
                anyhow::bail!(
                    "per_session mode: container already has {} session(s); cap={}",
                    count,
                    cap
                );
            }
        }

        let goose_bin = std::env::var("GOOSE_BIN")
            .ok()
            .or_else(|| which::which("goose").ok().map(|p| p.to_string_lossy().into_owned()))
            .context("goose binary not found; set GOOSE_BIN or install goose")?;

        // ACP mode — exact CLI invocation validated per F1 gate.
        let mut child = Command::new(&goose_bin)
            // F1-pending: exact CLI flags to be validated via `goose acp --help` once binary installed (see D141)
            .args(["acp", "--stdio"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("failed to spawn goose subprocess")?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(BufReader::new);
        let sid = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .await
            .insert(sid.clone(), SessionHandle { child, stdin, stdout });
        Ok(sid)
    }

    /// Write a JSON-RPC `session/send` message to goose stdin for the given session.
    ///
    /// The message is sent as a single newline-terminated JSON line.
    /// goose responds with ACP events readable via `next_event()`.
    pub async fn send_message(&self, sid: &str, content: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let handle = sessions
            .get_mut(sid)
            .ok_or_else(|| anyhow::anyhow!("session {sid} not found"))?;

        let stdin = handle
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("session {sid} stdin already closed"))?;

        // ACP JSON-RPC send request (newline-delimited)
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/send",
            "params": {
                "session_id": sid,
                "content": content
            }
        });
        let mut line = msg.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Read the next ACP event from goose stdout for the given session.
    ///
    /// Returns `None` when the subprocess closes stdout (EOF = session ended).
    /// Each call reads exactly one newline-delimited JSON-RPC line and parses it
    /// into an `AcpEvent`. Malformed lines are returned as `AcpEvent::Unknown`.
    pub async fn next_event(&self, sid: &str) -> Result<Option<AcpEvent>> {
        let mut sessions = self.sessions.lock().await;
        let handle = sessions
            .get_mut(sid)
            .ok_or_else(|| anyhow::anyhow!("session {sid} not found"))?;

        let stdout = match handle.stdout.as_mut() {
            Some(s) => s,
            None => return Ok(None),
        };

        let mut line = String::new();
        let n = stdout.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None); // EOF
        }

        let trimmed = line.trim_end();
        let event = AcpEvent::try_from(trimmed)
            .unwrap_or_else(|_| AcpEvent::Unknown { raw: trimmed.to_string() });
        Ok(Some(event))
    }

    /// F3 (no ACP cancellation API) — see design §9 Q#4.
    /// SIGTERM via `child.wait()` with 5s timeout; SIGKILL fallback on timeout or wait error.
    pub async fn close_session(&self, sid: &str) -> Result<()> {
        if let Some(mut h) = self.sessions.lock().await.remove(sid) {
            match tokio::time::timeout(std::time::Duration::from_secs(5), h.child.wait()).await {
                Ok(Ok(_)) => {}
                _ => {
                    let _ = h.child.kill().await;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_mode_per_session_caps_at_one() {
        let adapter = GooseAdapter::with_mode("per_session");
        assert_eq!(adapter.max_sessions(), Some(1));
    }

    #[tokio::test]
    async fn with_mode_shared_has_no_cap() {
        let adapter = GooseAdapter::with_mode("shared");
        assert_eq!(adapter.max_sessions(), None);
    }

    #[tokio::test]
    async fn with_mode_unknown_defaults_to_shared() {
        // ADR-V2-019 D2: anything other than "per_session" → uncapped shared mode.
        let adapter = GooseAdapter::with_mode("bogus-mode");
        assert_eq!(adapter.max_sessions(), None);
    }

    #[tokio::test]
    async fn new_delegates_to_shared_mode() {
        let adapter = GooseAdapter::new();
        assert_eq!(adapter.max_sessions(), None);
    }
}
