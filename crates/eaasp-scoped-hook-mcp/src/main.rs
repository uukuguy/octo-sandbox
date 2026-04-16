//! eaasp-scoped-hook-mcp — stdio MCP proxy entry point.
//!
//! IMPORTANT: All logging goes to stderr. stdout is the MCP wire.
//!
//! Environment variables:
//!   EAASP_DOWNSTREAM_CMD   — downstream MCP binary (required)
//!   EAASP_DOWNSTREAM_ARGS  — space-separated args to downstream (optional)
//!   EAASP_HOOK_DIR         — directory containing pre_tool_use.sh / post_tool_use.sh
//!   EAASP_SESSION_ID       — session id to embed in hook envelopes
//!   EAASP_SKILL_ID         — skill id to embed in hook envelopes
//!   EAASP_HOOK_TIMEOUT_SECS — per-hook timeout (default 5)

use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use eaasp_scoped_hook_mcp::{DownstreamMcp, ProxyServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Log to stderr only — stdout is MCP wire.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("eaasp_scoped_hook_mcp=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    let downstream_cmd = std::env::var("EAASP_DOWNSTREAM_CMD")
        .unwrap_or_else(|_| {
            eprintln!("EAASP_DOWNSTREAM_CMD not set; defaulting to 'cat' (passthrough)");
            "cat".to_string()
        });

    let downstream_args: Vec<String> = std::env::var("EAASP_DOWNSTREAM_ARGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();

    let hook_dir: Option<PathBuf> = std::env::var("EAASP_HOOK_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir());

    let session_id = std::env::var("EAASP_SESSION_ID").unwrap_or_default();
    let skill_id = std::env::var("EAASP_SKILL_ID").unwrap_or_default();
    let hook_timeout_secs: u64 = std::env::var("EAASP_HOOK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    tracing::info!(
        cmd = %downstream_cmd,
        hook_dir = ?hook_dir,
        session_id = %session_id,
        skill_id = %skill_id,
        timeout = hook_timeout_secs,
        "eaasp-scoped-hook-mcp starting"
    );

    let downstream = Arc::new(
        DownstreamMcp::spawn(&downstream_cmd, &downstream_args)
            .map_err(|e| format!("failed to spawn downstream: {e}"))?,
    );

    let proxy = ProxyServer::new(downstream, hook_dir, session_id, skill_id, hook_timeout_secs);

    // Read stdin line by line, proxy each message.
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // EOF
        }
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }

        if let Some(response) = proxy.handle_message(trimmed).await {
            stdout.write_all(response.as_bytes()).await?;
            if !response.ends_with('\n') {
                stdout.write_all(b"\n").await?;
            }
            stdout.flush().await?;
        }
    }

    Ok(())
}
