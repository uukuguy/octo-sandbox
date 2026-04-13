use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::config::{McpServerDef, RunMode};

/// A running MCP server process.
struct RunningProcess {
    child: Child,
    pid: u32,
    started_at: DateTime<Utc>,
}

/// Status information for a single MCP server.
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub transport: String,
    pub port: u16,
    pub mode: RunMode,
    pub tags: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
}

/// Manages MCP server subprocess lifecycle.
#[derive(Clone)]
pub struct McpManager {
    config: Vec<McpServerDef>,
    processes: Arc<Mutex<HashMap<String, RunningProcess>>>,
}

impl McpManager {
    /// Create a new manager with the given server definitions.
    pub fn new(config: Vec<McpServerDef>) -> Self {
        Self {
            config,
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// List all configured servers with their current status.
    pub async fn list_servers(&self) -> Vec<ServerStatus> {
        let procs = self.processes.lock().await;
        self.config
            .iter()
            .map(|def| {
                let running = procs.get(&def.name);
                ServerStatus {
                    name: def.name.clone(),
                    running: running.is_some(),
                    pid: running.map(|r| r.pid),
                    transport: def.transport.clone(),
                    port: def.port,
                    mode: def.mode,
                    tags: def.tags.clone(),
                    started_at: running.map(|r| r.started_at),
                }
            })
            .collect()
    }

    /// Start a server by name.
    pub async fn start(&self, name: &str) -> Result<()> {
        let def = self
            .config
            .iter()
            .find(|d| d.name == name)
            .ok_or_else(|| anyhow::anyhow!("server '{}' not found in config", name))?
            .clone();

        let mut procs = self.processes.lock().await;
        if procs.contains_key(name) {
            bail!("server '{}' is already running", name);
        }

        let mut cmd = Command::new(&def.command);
        cmd.args(&def.args);
        for (k, v) in &def.env {
            cmd.env(k, v);
        }
        // Detach stdio so the child doesn't block on pipe buffers.
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);

        procs.insert(
            name.to_string(),
            RunningProcess {
                child,
                pid,
                started_at: Utc::now(),
            },
        );

        tracing::info!(server = name, pid, "started MCP server");
        Ok(())
    }

    /// Stop a running server by name.
    pub async fn stop(&self, name: &str) -> Result<()> {
        let mut procs = self.processes.lock().await;
        let mut proc = procs
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("server '{}' is not running", name))?;

        proc.child.kill().await?;
        tracing::info!(server = name, "stopped MCP server");
        Ok(())
    }

    /// Get status for a single server.
    pub async fn get_info(&self, name: &str) -> Option<ServerStatus> {
        let procs = self.processes.lock().await;
        self.config.iter().find(|d| d.name == name).map(|def| {
            let running = procs.get(&def.name);
            ServerStatus {
                name: def.name.clone(),
                running: running.is_some(),
                pid: running.map(|r| r.pid),
                transport: def.transport.clone(),
                port: def.port,
                mode: def.mode,
                tags: def.tags.clone(),
                started_at: running.map(|r| r.started_at),
            }
        })
    }

    /// List servers that match any of the given tags.
    pub async fn list_by_tags(&self, tags: &[&str]) -> Vec<ServerStatus> {
        let procs = self.processes.lock().await;
        self.config
            .iter()
            .filter(|def| def.tags.iter().any(|t| tags.contains(&t.as_str())))
            .map(|def| {
                let running = procs.get(&def.name);
                ServerStatus {
                    name: def.name.clone(),
                    running: running.is_some(),
                    pid: running.map(|r| r.pid),
                    transport: def.transport.clone(),
                    port: def.port,
                    mode: def.mode,
                    tags: def.tags.clone(),
                    started_at: running.map(|r| r.started_at),
                }
            })
            .collect()
    }

    /// Start all servers configured with `Shared` mode.
    pub async fn start_all(&self) -> Result<()> {
        let shared_names: Vec<String> = self
            .config
            .iter()
            .filter(|d| d.mode == RunMode::Shared)
            .map(|d| d.name.clone())
            .collect();

        for name in shared_names {
            self.start(&name).await?;
        }
        Ok(())
    }

    /// Resolve skill dependencies to MCP server configs.
    ///
    /// Input: list of dependency strings like `"mcp:mock-scada"`, `"mcp:eaasp-l2-memory"`.
    /// Non-`mcp:` prefixed entries are silently ignored.
    /// Returns: matching `McpServerDef` entries in input order.
    pub fn resolve_dependencies(&self, dependencies: &[String]) -> Vec<&McpServerDef> {
        dependencies
            .iter()
            .filter_map(|dep| {
                let name = dep.strip_prefix("mcp:")?;
                self.config.iter().find(|s| s.name == name)
            })
            .collect()
    }

    /// Stop all running servers.
    pub async fn stop_all(&self) -> Result<()> {
        let names: Vec<String> = {
            let procs = self.processes.lock().await;
            procs.keys().cloned().collect()
        };

        for name in names {
            self.stop(&name).await?;
        }
        Ok(())
    }
}
