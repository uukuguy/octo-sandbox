use serde::Serialize;

use octo_types::{ChatMessage, RiskLevel, StopReason};

/// Events sent from AgentLoop to consumers (WebSocket handler, CLI, etc.)
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    TextDelta {
        text: String,
    },
    TextComplete {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ThinkingComplete {
        text: String,
    },
    ToolStart {
        tool_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_id: String,
        output: String,
        success: bool,
    },
    ToolExecution {
        execution: octo_types::ToolExecution,
    },
    TokenBudgetUpdate {
        budget: octo_types::TokenBudgetSnapshot,
    },
    Typing {
        /// true = started, false = stopped
        state: bool,
    },
    Error {
        message: String,
    },
    Done,
    ContextDegraded {
        level: String,
        usage_pct: f32,
    },
    MemoryFlushed {
        facts_count: usize,
    },
    ApprovalRequired {
        tool_name: String,
        tool_id: String,
        risk_level: RiskLevel,
    },
    SecurityBlocked {
        reason: String,
    },
    IterationStart {
        round: u32,
    },
    IterationEnd {
        round: u32,
        /// Cumulative input tokens so far in this agent loop.
        input_tokens: u64,
        /// Cumulative output tokens so far in this agent loop.
        output_tokens: u64,
    },
    /// Tool execution progress update (for long-running tools).
    ToolProgress {
        tool_id: String,
        tool_name: String,
        progress: octo_types::ToolProgress,
    },
    Completed(AgentLoopResult),
    /// Plan steps updated (from dual-mode agent).
    PlanUpdate {
        steps: Vec<super::dual::PlanStep>,
    },
    /// The agent loop was halted by an emergency stop (E-Stop).
    EmergencyStopped(Option<String>),
    /// Streaming event from a sub-agent (e.g. playbook skill execution).
    /// Wrapped to isolate sub-agent state from parent agent state in the TUI.
    SubAgentEvent {
        /// Human-readable sub-agent identifier (e.g. "skill-review").
        source_id: String,
        /// The original event from the sub-agent.
        inner: Box<AgentEvent>,
    },
}

/// Structured return result for AgentLoop (Opus §3.2)
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentLoopResult {
    pub rounds: u32,
    pub tool_calls: u32,
    pub stop_reason: NormalizedStopReason,
    /// Total input tokens consumed across all rounds
    pub input_tokens: u64,
    /// Total output tokens consumed across all rounds
    pub output_tokens: u64,
    /// Final conversation messages after the agent loop completes (D1).
    /// Enables consumers to access the full message history without
    /// reconstructing it from the event stream.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub final_messages: Vec<ChatMessage>,
}

/// Normalized stop reason (ZeroClaw pattern) — covers all agent-level stop reasons.
/// This is distinct from octo_types::StopReason which is provider-level.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub enum NormalizedStopReason {
    #[default]
    EndTurn,
    ToolCall,
    MaxTokens,
    MaxIterations,
    ContextOverflow,
    SafetyBlocked,
    Cancelled,
    Error,
}

impl From<StopReason> for NormalizedStopReason {
    fn from(sr: StopReason) -> Self {
        match sr {
            StopReason::EndTurn => Self::EndTurn,
            StopReason::ToolUse => Self::ToolCall,
            StopReason::MaxTokens => Self::MaxTokens,
            StopReason::StopSequence => Self::EndTurn,
        }
    }
}

impl From<Option<StopReason>> for NormalizedStopReason {
    fn from(sr: Option<StopReason>) -> Self {
        match sr {
            Some(r) => r.into(),
            None => Self::EndTurn,
        }
    }
}

impl NormalizedStopReason {
    /// Parse from a raw string (as returned by some providers).
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "end_turn" | "stop" => Self::EndTurn,
            "tool_use" | "tool_calls" => Self::ToolCall,
            "max_tokens" | "length" => Self::MaxTokens,
            "stop_sequence" | "content_filter" => Self::EndTurn,
            _ => Self::EndTurn,
        }
    }

    /// Whether this stop reason indicates the turn is complete
    /// (no further processing needed).
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::ToolCall | Self::MaxTokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_progress_event_serialization() {
        let event = AgentEvent::ToolProgress {
            tool_id: "call_123".into(),
            tool_name: "bash".into(),
            progress: octo_types::ToolProgress::percent(0.5, "Processing...")
                .with_elapsed(1500),
        };
        let json = serde_json::to_value(&event).expect("serialization should succeed");
        assert_eq!(json["type"], "ToolProgress");
        assert_eq!(json["tool_id"], "call_123");
        assert_eq!(json["tool_name"], "bash");
        assert_eq!(json["progress"]["fraction"], 0.5);
        assert_eq!(json["progress"]["message"], "Processing...");
        assert_eq!(json["progress"]["elapsed_ms"], 1500);
    }

    #[test]
    fn test_tool_progress_complete_flag() {
        let progress = octo_types::ToolProgress::percent(1.0, "Done");
        assert!(progress.is_complete());

        let progress_half = octo_types::ToolProgress::percent(0.5, "Half");
        assert!(!progress_half.is_complete());

        let indeterminate = octo_types::ToolProgress::indeterminate("Working...");
        assert!(!indeterminate.is_complete());
    }
}
