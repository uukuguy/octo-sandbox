use serde::{Deserialize, Serialize};

// ============================================================
// Memory Type (Cognitive 3-layer model)
// ============================================================

/// Memory type based on cognitive science's three-layer memory model.
///
/// - Semantic: facts, preferences, knowledge ("what I know")
/// - Episodic: events, conversation summaries, timelines ("what I experienced")
/// - Procedural: workflow patterns, best practices ("how I do things")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Semantic,
    Episodic,
    Procedural,
}

impl Default for MemoryType {
    fn default() -> Self {
        Self::Semantic
    }
}

impl MemoryType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Semantic => "semantic",
            Self::Episodic => "episodic",
            Self::Procedural => "procedural",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "semantic" => Some(Self::Semantic),
            "episodic" => Some(Self::Episodic),
            "procedural" => Some(Self::Procedural),
            _ => None,
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================
// Event Data (structured event from tool call chains)
// ============================================================

/// Structured event data extracted from tool call chains.
///
/// Captures the key elements of an action: what happened, to what,
/// with what result, and what artifacts were produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    /// Event type: register, create, delete, deploy, configure, etc.
    pub event_type: String,
    /// Event target: "MoltBook website", "database schema", etc.
    pub target: String,
    /// Outcome: success, failure, partial
    pub outcome: String,
    /// Key artifacts: {"username": "octo-agent", "email": "..."}
    pub artifacts: serde_json::Value,
    /// Tool chain used to accomplish this event
    pub tool_chain: Vec<String>,
}

impl EventData {
    pub fn new(
        event_type: impl Into<String>,
        target: impl Into<String>,
        outcome: impl Into<String>,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            target: target.into(),
            outcome: outcome.into(),
            artifacts: serde_json::json!({}),
            tool_chain: Vec::new(),
        }
    }

    pub fn with_artifacts(mut self, artifacts: serde_json::Value) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_tool_chain(mut self, tools: Vec<String>) -> Self {
        self.tool_chain = tools;
        self
    }
}

// ============================================================
// Sort Field (for memory queries)
// ============================================================

/// Sort field for memory search/list queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    Relevance,
    CreatedAt,
    UpdatedAt,
    Importance,
}

// ============================================================
// Working Memory (Layer 0)
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBlockKind {
    #[deprecated(note = "Use SystemPromptBuilder capabilities section instead")]
    SandboxContext,
    #[deprecated(note = "Use AgentManifest for agent identity")]
    AgentPersona,
    UserProfile,
    TaskContext,
    AutoExtracted,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlock {
    pub id: String,
    pub kind: MemoryBlockKind,
    pub label: String,
    pub value: String,
    pub priority: u8,
    pub max_age_turns: Option<u32>,
    pub last_updated_turn: u32,
    pub char_limit: usize,
    pub is_readonly: bool,
}

impl MemoryBlock {
    #[allow(deprecated)]
    pub fn new(kind: MemoryBlockKind, label: impl Into<String>, value: impl Into<String>) -> Self {
        let kind_str = match &kind {
            MemoryBlockKind::SandboxContext => "sandbox_context",
            MemoryBlockKind::AgentPersona => "agent_persona",
            MemoryBlockKind::UserProfile => "user_profile",
            MemoryBlockKind::TaskContext => "task_context",
            MemoryBlockKind::AutoExtracted => "auto_extracted",
            MemoryBlockKind::Custom => "custom",
        };
        Self {
            id: kind_str.to_string(),
            kind,
            label: label.into(),
            value: value.into(),
            priority: 128,
            max_age_turns: None,
            last_updated_turn: 0,
            char_limit: 2000,
            is_readonly: false,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_max_age(mut self, turns: u32) -> Self {
        self.max_age_turns = Some(turns);
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn char_count(&self) -> usize {
        self.value.len()
    }

    pub fn is_expired(&self, current_turn: u32) -> bool {
        match self.max_age_turns {
            Some(max) => current_turn.saturating_sub(self.last_updated_turn) > max,
            None => false,
        }
    }
}

// ============================================================
// Token Budget (Zone A/B/C)
// ============================================================

/// Token budget for context zone allocation.
/// - Zone A: system_prompt (system instructions)
/// - Zone B: context (dynamic context from memory/injector)
/// - Zone C: conversation (history messages)
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub total: u32,
    pub system_prompt: u32, // Zone A
    pub context: u32,       // Zone B: aligned with ContextInjector
    pub conversation: u32,  // Zone C
    pub completion: u32,    // Reserve for model output
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            total: 200_000,
            system_prompt: 16_000, // Zone A: 16K
            context: 12_000,       // Zone B: 12K (aligned with ContextInjector)
            conversation: 32_000,  // Zone C: 32K
            completion: 4_096,
        }
    }
}

/// Default context budget in characters (Zone B).
/// Must be kept in sync with TokenBudget::default().context * 4 (chars ≈ 4x tokens).
pub const DEFAULT_CONTEXT_BUDGET_CHARS: usize = 12_000;

// ============================================================
// Persistent Memory (Layer 2)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryId(pub String);

impl MemoryId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Profile,
    Preferences,
    Tools,
    Debug,
    Patterns,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Profile => "profile",
            Self::Preferences => "preferences",
            Self::Tools => "tools",
            Self::Debug => "debug",
            Self::Patterns => "patterns",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "profile" => Some(Self::Profile),
            "preferences" => Some(Self::Preferences),
            "tools" => Some(Self::Tools),
            "debug" => Some(Self::Debug),
            "patterns" => Some(Self::Patterns),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    Extracted,
    Manual,
    System,
}

impl MemorySource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Extracted => "extracted",
            Self::Manual => "manual",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "extracted" => Self::Extracted,
            "manual" => Self::Manual,
            "system" => Self::System,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimestamps {
    pub created_at: i64,
    pub updated_at: i64,
    pub accessed_at: i64,
}

impl Default for MemoryTimestamps {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            created_at: now,
            updated_at: now,
            accessed_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub user_id: String,
    pub sandbox_id: String,
    pub category: MemoryCategory,
    pub content: String,
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
    pub importance: f32,
    pub access_count: u32,
    pub source_type: MemorySource,
    pub source_ref: String,
    pub ttl: Option<i64>,
    pub timestamps: MemoryTimestamps,
    /// Memory type (semantic/episodic/procedural)
    pub memory_type: MemoryType,
    /// Source session ID (for episodic tracing)
    pub session_id: Option<String>,
    /// Structured event data (for episodic events)
    pub event_data: Option<EventData>,
}

impl MemoryEntry {
    pub fn new(
        user_id: impl Into<String>,
        category: MemoryCategory,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: MemoryId::new(),
            user_id: user_id.into(),
            sandbox_id: String::new(),
            category,
            content: content.into(),
            metadata: serde_json::json!({}),
            embedding: None,
            importance: 0.5,
            access_count: 0,
            source_type: MemorySource::Manual,
            source_ref: String::new(),
            ttl: None,
            timestamps: MemoryTimestamps::default(),
            memory_type: MemoryType::default(),
            session_id: None,
            event_data: None,
        }
    }

    /// Create an episodic memory entry from event data.
    pub fn new_episodic(
        user_id: impl Into<String>,
        event: &EventData,
        session_id: impl Into<String>,
    ) -> Self {
        let content = format!("{}: {} — {}", event.event_type, event.target, event.outcome);
        Self {
            id: MemoryId::new(),
            user_id: user_id.into(),
            sandbox_id: String::new(),
            category: MemoryCategory::Profile,
            content,
            metadata: serde_json::json!({
                "event_type": event.event_type,
                "target": event.target,
            }),
            embedding: None,
            importance: 0.7,
            access_count: 0,
            source_type: MemorySource::Extracted,
            source_ref: format!("event:{}", event.event_type),
            ttl: None,
            timestamps: MemoryTimestamps::default(),
            memory_type: MemoryType::Episodic,
            session_id: Some(session_id.into()),
            event_data: Some(event.clone()),
        }
    }

    /// Create a procedural memory entry (workflow pattern).
    pub fn new_procedural(
        user_id: impl Into<String>,
        description: &str,
        tool_sequence: &[String],
        task_type: &str,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            id: MemoryId::new(),
            user_id: user_id.into(),
            sandbox_id: String::new(),
            category: MemoryCategory::Patterns,
            content: description.to_string(),
            metadata: serde_json::json!({
                "tool_sequence": tool_sequence,
                "task_type": task_type,
            }),
            embedding: None,
            importance: 0.6,
            access_count: 0,
            source_type: MemorySource::Extracted,
            source_ref: format!("procedural:{task_type}"),
            ttl: None,
            timestamps: MemoryTimestamps::default(),
            memory_type: MemoryType::Procedural,
            session_id: Some(session_id.into()),
            event_data: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub user_id: String,
    pub sandbox_id: Option<String>,
    pub categories: Option<Vec<MemoryCategory>>,
    pub limit: usize,
    pub token_budget: usize,
    pub min_score: Option<f32>,
    pub time_decay: bool,
    pub query_embedding: Option<Vec<f32>>,
    /// Time range filter (start_timestamp, end_timestamp)
    pub time_range: Option<(i64, i64)>,
    /// Filter by source session
    pub session_id: Option<String>,
    /// Filter by memory types
    pub memory_types: Option<Vec<MemoryType>>,
    /// Sort field (default: Relevance)
    pub sort_by: SortField,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            sandbox_id: None,
            categories: None,
            limit: 20,
            token_budget: 8000,
            min_score: None,
            time_decay: true,
            query_embedding: None,
            time_range: None,
            session_id: None,
            memory_types: None,
            sort_by: SortField::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub entry: MemoryEntry,
    pub score: f32,
    pub match_source: String,
}

#[derive(Debug, Clone)]
pub struct MemoryFilter {
    pub user_id: String,
    pub sandbox_id: Option<String>,
    pub categories: Option<Vec<MemoryCategory>>,
    pub source_types: Option<Vec<MemorySource>>,
    pub limit: usize,
    /// Time range filter (start_timestamp, end_timestamp)
    pub time_range: Option<(i64, i64)>,
    /// Filter by source session
    pub session_id: Option<String>,
    /// Filter by memory types
    pub memory_types: Option<Vec<MemoryType>>,
    /// Maximum importance threshold — matches entries with importance <= this value
    pub max_importance: Option<f32>,
    /// Maximum access count — matches entries accessed <= this many times
    pub max_access_count: Option<u32>,
    /// Minimum age in seconds — matches entries older than (now - older_than_secs)
    pub older_than_secs: Option<i64>,
}

impl Default for MemoryFilter {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            sandbox_id: None,
            categories: None,
            source_types: None,
            limit: 50,
            time_range: None,
            session_id: None,
            memory_types: None,
            max_importance: None,
            max_access_count: None,
            older_than_secs: None,
        }
    }
}
