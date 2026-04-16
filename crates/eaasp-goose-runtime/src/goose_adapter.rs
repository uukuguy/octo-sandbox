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
}

impl Default for GooseAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GooseAdapter {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start_session(&self, _cfg: SessionConfig) -> Result<String> {
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
