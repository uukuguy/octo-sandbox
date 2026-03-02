//! AgentEntry and AgentManifest - core registry types

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
    pub manifest: AgentManifest,
    pub state: AgentStatus,
    pub created_at: i64,
}

impl AgentEntry {
    pub fn new(manifest: AgentManifest) -> Self {
        Self {
            id: AgentId::new(),
            manifest,
            state: AgentStatus::Created,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }
}
