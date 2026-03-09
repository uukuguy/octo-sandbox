/// Structured return result for AgentLoop (Opus §3.2)
#[derive(Debug, Clone, Default)]
pub struct AgentLoopResult {
    pub rounds: u32,
    pub tool_calls: u32,
    pub stop_reason: NormalizedStopReason,
}

/// Normalized stop reason (ZeroClaw pattern) — covers all agent-level stop reasons.
/// This is distinct from octo_types::StopReason which is provider-level.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
