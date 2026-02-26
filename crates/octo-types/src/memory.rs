use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBlockKind {
    SandboxContext,
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
}

impl MemoryBlock {
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

#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub total: u32,
    pub system_prompt: u32,
    pub memory: u32,
    pub messages: u32,
    pub completion: u32,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            total: 200_000,
            system_prompt: 4_000,
            memory: 2_000,
            messages: 180_000,
            completion: 4_096,
        }
    }
}
