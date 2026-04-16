// Outcome B: adapter owns a subprocess Child per session + ACP client handle.
// T2 wires the subprocess spawn + ACP placeholder; T3 will add the real ACP
// client type + middleware hook wiring.
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

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
    // acp_client: AcpClient,   // placeholder; T3 resolves real ACP client type
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
        let child = Command::new(&goose_bin)
            // F1-pending: exact CLI flags to be validated via `goose acp --help` once binary installed (see D141)
            .args(["acp", "--stdio"])
            .kill_on_drop(true)
            .spawn()
            .context("failed to spawn goose subprocess")?;

        let sid = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .await
            .insert(sid.clone(), SessionHandle { child });
        Ok(sid)
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
