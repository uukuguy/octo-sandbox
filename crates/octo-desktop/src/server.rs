//! Embedded HTTP server for Octo Desktop.
//!
//! Starts an Axum server on a random port, serving the Dashboard UI.
//! The WebView connects to this local server.

use anyhow::Result;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::net::TcpListener;
use tracing::info;

/// Global atomic holding the bound server port, readable from Tauri IPC commands.
pub static SERVER_PORT: AtomicU16 = AtomicU16::new(0);

/// Server state holding the bound port.
pub struct EmbeddedServer {
    pub port: u16,
}

impl EmbeddedServer {
    /// Start the embedded Axum server on a random port.
    ///
    /// Binds to `127.0.0.1:0` (OS-assigned port), builds the dashboard router
    /// from `octo-cli`, and spawns the server as a background tokio task.
    /// The bound port is stored in [`SERVER_PORT`] for IPC access.
    pub async fn start() -> Result<Self> {
        // Build the dashboard router — no auth for local desktop use
        let router = octo_cli::commands::dashboard::build_router(false, &[], None);

        // Bind to a random available port on localhost
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        // Store the port globally for IPC commands
        SERVER_PORT.store(port, Ordering::SeqCst);

        info!(port, "Embedded dashboard server starting");

        // Spawn the server as a background task
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(error = %e, "Embedded dashboard server error");
            }
        });

        Ok(Self { port })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_port_initial_state() {
        // SERVER_PORT starts at 0 before any server is started.
        // Note: if a previous test already started the server in this process,
        // the atomic may have been set. We just verify it's a valid u16.
        let port = SERVER_PORT.load(Ordering::SeqCst);
        // Port is either 0 (not started) or a valid ephemeral port
        assert!(port == 0 || port >= 1024);
    }

    #[tokio::test]
    async fn embedded_server_starts_and_binds_port() {
        let server = EmbeddedServer::start().await.expect("server should start");
        assert!(server.port > 0, "server should bind to a port > 0");

        let stored = SERVER_PORT.load(Ordering::SeqCst);
        assert_eq!(stored, server.port, "global SERVER_PORT should match");
    }

    #[tokio::test]
    async fn embedded_server_responds_to_requests() {
        let server = EmbeddedServer::start().await.expect("server should start");

        // Give the spawned task a moment to accept connections
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Use a client with no proxy to avoid 502 from system proxy
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client should build");

        let url = format!("http://127.0.0.1:{}/", server.port);
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("HTTP request should succeed");
        assert!(
            resp.status().is_success(),
            "root endpoint should return 2xx, got {}",
            resp.status()
        );
    }
}
