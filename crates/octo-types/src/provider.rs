use serde::{Deserialize, Serialize};

use crate::message::{ChatMessage, ContentBlock};
use crate::tool::ToolSpec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub tools: Vec<ToolSpec>,
    pub stream: bool,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".into(),
            system: None,
            messages: Vec::new(),
            max_tokens: 4096,
            temperature: None,
            tools: Vec::new(),
            stream: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<StopReason>,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageStart {
        id: String,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ToolUseStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolUseInputDelta {
        index: usize,
        partial_json: String,
    },
    ToolUseComplete {
        index: usize,
        id: String,
        name: String,
        input: serde_json::Value,
    },
    MessageStop {
        stop_reason: StopReason,
        usage: TokenUsage,
    },
}
