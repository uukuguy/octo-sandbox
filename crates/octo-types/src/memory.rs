use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBlockKind {
    SandboxContext,
    AgentPersona,
    UserProfile,
    TaskContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlock {
    pub id: String,
    pub kind: MemoryBlockKind,
    pub label: String,
    pub value: String,
}

impl MemoryBlock {
    pub fn new(kind: MemoryBlockKind, label: impl Into<String>, value: impl Into<String>) -> Self {
        let kind_str = match &kind {
            MemoryBlockKind::SandboxContext => "sandbox_context",
            MemoryBlockKind::AgentPersona => "agent_persona",
            MemoryBlockKind::UserProfile => "user_profile",
            MemoryBlockKind::TaskContext => "task_context",
        };
        Self {
            id: kind_str.to_string(),
            kind,
            label: label.into(),
            value: value.into(),
        }
    }

    pub fn char_count(&self) -> usize {
        self.value.len()
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
