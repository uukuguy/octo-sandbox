//! TUI backend abstraction
//!
//! Provides a unified interface for TUI screens to interact with the engine,
//! whether running in-process or connecting to a remote server.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use octo_engine::AgentRuntime;

// ---------------------------------------------------------------------------
// Display-oriented summary types
// ---------------------------------------------------------------------------

/// Summary of an agent for display.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub role: String,
    pub status: String,
}

/// Summary of a session for display.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub message_count: usize,
}

/// Summary of an MCP server for display.
#[derive(Debug, Clone)]
pub struct McpServerSummary {
    pub name: String,
    pub status: String,
    pub tool_count: usize,
}

/// Summary of a tool for display.
#[derive(Debug, Clone)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
}

/// Summary of a memory entry for display.
#[derive(Debug, Clone)]
pub struct MemorySummary {
    pub id: String,
    pub content: String,
    pub category: String,
    pub score: Option<f32>,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// TUI backend trait -- abstracts engine access for screens.
#[async_trait]
pub trait TuiBackend: Send + Sync {
    /// List all registered agents.
    async fn list_agents(&self) -> Result<Vec<AgentSummary>>;

    /// List recent sessions (up to `limit`).
    async fn list_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>>;

    /// Search persistent memory.
    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemorySummary>>;

    /// List connected MCP servers.
    async fn list_mcp_servers(&self) -> Result<Vec<McpServerSummary>>;

    /// List available tools.
    async fn list_tools(&self) -> Result<Vec<ToolSummary>>;
}

// ---------------------------------------------------------------------------
// InProcessBackend
// ---------------------------------------------------------------------------

/// In-process backend -- wraps `Arc<AgentRuntime>` directly.
pub struct InProcessBackend {
    runtime: Arc<AgentRuntime>,
}

impl InProcessBackend {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl TuiBackend for InProcessBackend {
    async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        let entries = self.runtime.catalog().list_all();
        Ok(entries
            .into_iter()
            .map(|e| AgentSummary {
                id: e.id.to_string(),
                name: e.manifest.name.clone(),
                role: e.manifest.role.clone().unwrap_or_default(),
                status: e.state.to_string(),
            })
            .collect())
    }

    async fn list_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let sessions = self.runtime.session_store().list_sessions(limit, 0).await;
        Ok(sessions
            .into_iter()
            .map(|s| SessionSummary {
                id: s.session_id,
                created_at: chrono::DateTime::from_timestamp(s.created_at, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| s.created_at.to_string()),
                message_count: s.message_count,
            })
            .collect())
    }

    async fn search_memory(&self, _query: &str, _limit: usize) -> Result<Vec<MemorySummary>> {
        // Memory search requires embedding generation which is not available here.
        // Future: wire up MemoryStore::search when embedding provider is accessible.
        Ok(vec![])
    }

    async fn list_mcp_servers(&self) -> Result<Vec<McpServerSummary>> {
        let mgr = self.runtime.mcp_manager().lock().await;
        let states = mgr.all_runtime_states();
        Ok(states
            .into_iter()
            .map(|(name, state)| {
                let tool_count = mgr.get_tool_count(&name);
                McpServerSummary {
                    name,
                    status: format!("{:?}", state),
                    tool_count,
                }
            })
            .collect())
    }

    async fn list_tools(&self) -> Result<Vec<ToolSummary>> {
        let guard = self
            .runtime
            .tools()
            .lock()
            .map_err(|e| anyhow::anyhow!("poisoned lock: {}", e))?;
        let specs = guard.specs();
        Ok(specs
            .into_iter()
            .map(|s| ToolSummary {
                name: s.name,
                description: s.description,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// HttpBackend (stub)
// ---------------------------------------------------------------------------

/// HTTP backend stub -- connects to a remote octo-server (future implementation).
pub struct HttpBackend {
    #[allow(dead_code)]
    base_url: String,
}

impl HttpBackend {
    #[allow(dead_code)]
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

#[async_trait]
impl TuiBackend for HttpBackend {
    async fn list_agents(&self) -> Result<Vec<AgentSummary>> {
        anyhow::bail!("HTTP backend not yet implemented")
    }

    async fn list_sessions(&self, _limit: usize) -> Result<Vec<SessionSummary>> {
        anyhow::bail!("HTTP backend not yet implemented")
    }

    async fn search_memory(&self, _query: &str, _limit: usize) -> Result<Vec<MemorySummary>> {
        anyhow::bail!("HTTP backend not yet implemented")
    }

    async fn list_mcp_servers(&self) -> Result<Vec<McpServerSummary>> {
        anyhow::bail!("HTTP backend not yet implemented")
    }

    async fn list_tools(&self) -> Result<Vec<ToolSummary>> {
        anyhow::bail!("HTTP backend not yet implemented")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_summary_debug_and_clone() {
        let s = AgentSummary {
            id: "id-1".into(),
            name: "coder".into(),
            role: "developer".into(),
            status: "running".into(),
        };
        let s2 = s.clone();
        assert_eq!(s2.id, "id-1");
        assert_eq!(s2.name, "coder");
        assert_eq!(format!("{:?}", s2), format!("{:?}", s));
    }

    #[test]
    fn session_summary_fields() {
        let s = SessionSummary {
            id: "sess-abc".into(),
            created_at: "2026-03-10 12:00:00".into(),
            message_count: 5,
        };
        assert_eq!(s.message_count, 5);
        assert!(s.created_at.contains("2026"));
    }

    #[test]
    fn mcp_server_summary_fields() {
        let s = McpServerSummary {
            name: "test-server".into(),
            status: "Running { pid: 42 }".into(),
            tool_count: 3,
        };
        assert_eq!(s.tool_count, 3);
        assert!(s.status.contains("Running"));
    }

    #[test]
    fn tool_summary_fields() {
        let s = ToolSummary {
            name: "bash".into(),
            description: "Execute bash commands".into(),
        };
        assert_eq!(s.name, "bash");
    }

    #[test]
    fn memory_summary_optional_score() {
        let s = MemorySummary {
            id: "mem-1".into(),
            content: "test content".into(),
            category: "general".into(),
            score: Some(0.95),
        };
        assert_eq!(s.score, Some(0.95));

        let s2 = MemorySummary {
            id: "mem-2".into(),
            content: "no score".into(),
            category: "".into(),
            score: None,
        };
        assert!(s2.score.is_none());
    }

    #[test]
    fn http_backend_construction() {
        let backend = HttpBackend::new("http://localhost:3001".into());
        assert_eq!(backend.base_url, "http://localhost:3001");
    }
}
