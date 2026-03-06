use serde::{Deserialize, Serialize};

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

    pub fn from_str(s: &str) -> Option<Self> {
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

    pub fn from_str(s: &str) -> Self {
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
}

impl Default for MemoryFilter {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            sandbox_id: None,
            categories: None,
            source_types: None,
            limit: 50,
        }
    }
}
