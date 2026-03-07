//! AgentEntry and AgentManifest - core registry types

use octo_types::{TenantId, DEFAULT_TENANT_ID};
use serde::{Deserialize, Serialize};

use crate::agent::AgentConfig;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent specification - provided at creation time, defines identity and behavior.
///
/// System prompt priority:
///   system_prompt > role/goal/backstory > SOUL.md > CORE_INSTRUCTIONS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,

    // Identity (three-part, CrewAI pattern)
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub backstory: Option<String>,

    // Full system prompt override (highest priority, skips role/goal/backstory)
    #[serde(default)]
    pub system_prompt: Option<String>,

    // Runtime overrides
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_filter: Vec<String>, // empty = all tools available
    #[serde(default)]
    pub config: AgentConfig,
    /// Maximum number of concurrent tasks (0 = unlimited)
    #[serde(default)]
    pub max_concurrent_tasks: u32,
    /// Priority level hint (e.g. "high", "medium", "low")
    #[serde(default)]
    pub priority: Option<String>,
}

impl AgentManifest {
    /// Build an AgentProfile from this manifest for use with AgentRouter.
    /// Capabilities are inferred from tags using "cap:" prefix convention.
    pub fn to_agent_profile(&self, agent_id: impl Into<String>) -> crate::agent::router::AgentProfile {
        use crate::agent::capability::AgentCapability;
        use crate::agent::router::AgentProfile;

        let capabilities: Vec<AgentCapability> = if self.tags.is_empty() {
            vec![AgentCapability::General]
        } else {
            let caps: Vec<AgentCapability> = self.tags.iter()
                .filter(|t| t.starts_with("cap:"))
                .map(|t| AgentCapability::from_str_loose(&t[4..]))
                .collect();
            if caps.is_empty() {
                vec![AgentCapability::General]
            } else {
                caps
            }
        };

        AgentProfile {
            agent_id: agent_id.into(),
            capabilities,
            priority: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Error(String),
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Created
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error(e) => write!(f, "error: {e}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub id: AgentId,
    pub tenant_id: TenantId,
    pub manifest: AgentManifest,
    pub state: AgentStatus,
    pub created_at: i64,
}

impl AgentEntry {
    pub fn new(manifest: AgentManifest, tenant_id: Option<TenantId>) -> Self {
        let tenant_id = tenant_id.unwrap_or_else(|| TenantId::from_string(DEFAULT_TENANT_ID));
        Self {
            id: AgentId::new(),
            tenant_id,
            manifest,
            state: AgentStatus::Created,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }
}

#[derive(Debug)]
pub enum AgentError {
    NotFound(AgentId),
    InvalidTransition {
        from: AgentStatus,
        action: &'static str,
    },
    ScheduledTask(String),
    Internal(String),

    // MCP-related errors
    McpNotInitialized,
    McpError(String),
    McpServerNotFound(String),

    // Permission-related errors (Task 3)
    PermissionDenied(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "agent not found: {id}"),
            Self::InvalidTransition { from, action } => {
                write!(f, "cannot {action} agent in state {from}")
            }
            Self::ScheduledTask(msg) => write!(f, "scheduled task error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),

            // MCP-related errors
            Self::McpNotInitialized => write!(f, "MCP manager not initialized"),
            Self::McpError(msg) => write!(f, "MCP error: {msg}"),
            Self::McpServerNotFound(name) => write!(f, "MCP server not found: {name}"),

            // Permission-related errors
            Self::PermissionDenied(msg) => write!(f, "permission denied: {msg}"),
        }
    }
}

impl std::error::Error for AgentError {}
