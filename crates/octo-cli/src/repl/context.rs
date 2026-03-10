//! REPL session context — mutable state shared across the REPL loop

pub use octo_engine::AgentSlot;

/// Operating mode for the REPL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMode {
    /// Plan mode — read-only, no tool execution allowed
    Plan,
    /// Build mode — full permissions (default)
    Build,
}

impl Default for ReplMode {
    fn default() -> Self {
        Self::Build
    }
}

impl std::fmt::Display for ReplMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan => write!(f, "plan"),
            Self::Build => write!(f, "build"),
        }
    }
}

impl ReplMode {
    /// Parse a mode string, returning `None` for unrecognized values.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "plan" => Some(Self::Plan),
            "build" => Some(Self::Build),
            _ => None,
        }
    }

    /// Description of what the mode does.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Plan => "read-only, no tool execution",
            Self::Build => "full permissions (default)",
        }
    }
}

/// Mutable context for the REPL session
pub struct ReplContext {
    /// Current operating mode
    pub mode: ReplMode,
    /// Total input tokens used in this session
    pub total_input_tokens: u64,
    /// Total output tokens used in this session
    pub total_output_tokens: u64,
    /// Number of conversation rounds completed
    pub rounds: u32,
    /// Number of tool calls made
    pub tool_calls: u32,
    /// Number of messages in the conversation
    pub message_count: usize,
    /// Active agent slot when in dual-agent mode (None = single agent mode)
    pub active_agent: Option<AgentSlot>,
    /// Whether auto-memory extraction is enabled
    pub auto_memory_enabled: bool,
    /// Number of auto-extracted memories in this session
    pub auto_memory_count: usize,
    /// Collaboration mode info (None = not in collab mode)
    pub collaboration_agents: Option<Vec<String>>,
}

impl Default for ReplContext {
    fn default() -> Self {
        Self {
            mode: ReplMode::default(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            rounds: 0,
            tool_calls: 0,
            message_count: 0,
            active_agent: None,
            auto_memory_enabled: true,
            auto_memory_count: 0,
            collaboration_agents: None,
        }
    }
}

impl ReplContext {
    /// Estimate the cost in USD based on Anthropic Claude 3.5 Sonnet pricing.
    ///
    /// Rough estimates: input $3/MTok, output $15/MTok.
    pub fn estimated_cost_usd(&self) -> f64 {
        let input_cost = self.total_input_tokens as f64 * 3.0 / 1_000_000.0;
        let output_cost = self.total_output_tokens as f64 * 15.0 / 1_000_000.0;
        input_cost + output_cost
    }

    /// Update context with result from an agent loop completion.
    pub fn record_completion(&mut self, rounds: u32, tool_calls: u32, input_tokens: u64, output_tokens: u64) {
        self.rounds += rounds;
        self.tool_calls += tool_calls;
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_mode_default_is_build() {
        assert_eq!(ReplMode::default(), ReplMode::Build);
    }

    #[test]
    fn test_repl_mode_display() {
        assert_eq!(format!("{}", ReplMode::Plan), "plan");
        assert_eq!(format!("{}", ReplMode::Build), "build");
    }

    #[test]
    fn test_repl_mode_from_str() {
        assert_eq!(ReplMode::from_str("plan"), Some(ReplMode::Plan));
        assert_eq!(ReplMode::from_str("build"), Some(ReplMode::Build));
        assert_eq!(ReplMode::from_str("Plan"), Some(ReplMode::Plan));
        assert_eq!(ReplMode::from_str("BUILD"), Some(ReplMode::Build));
        assert_eq!(ReplMode::from_str(" plan "), Some(ReplMode::Plan));
        assert_eq!(ReplMode::from_str("unknown"), None);
        assert_eq!(ReplMode::from_str(""), None);
    }

    #[test]
    fn test_repl_mode_description() {
        assert!(ReplMode::Plan.description().contains("read-only"));
        assert!(ReplMode::Build.description().contains("full permissions"));
    }

    #[test]
    fn test_repl_context_default() {
        let ctx = ReplContext::default();
        assert_eq!(ctx.mode, ReplMode::Build);
        assert_eq!(ctx.total_input_tokens, 0);
        assert_eq!(ctx.total_output_tokens, 0);
        assert_eq!(ctx.rounds, 0);
        assert_eq!(ctx.tool_calls, 0);
        assert_eq!(ctx.message_count, 0);
        assert_eq!(ctx.active_agent, None);
        assert!(ctx.auto_memory_enabled);
        assert_eq!(ctx.auto_memory_count, 0);
        assert!(ctx.collaboration_agents.is_none());
    }

    #[test]
    fn test_estimated_cost_zero() {
        let ctx = ReplContext::default();
        assert_eq!(ctx.estimated_cost_usd(), 0.0);
    }

    #[test]
    fn test_estimated_cost_calculation() {
        let mut ctx = ReplContext::default();
        ctx.total_input_tokens = 1_000_000;
        ctx.total_output_tokens = 1_000_000;
        // input: 1M * $3/M = $3, output: 1M * $15/M = $15 → total $18
        let cost = ctx.estimated_cost_usd();
        assert!((cost - 18.0).abs() < 0.001);
    }

    #[test]
    fn test_record_completion() {
        let mut ctx = ReplContext::default();
        ctx.record_completion(3, 5, 1000, 500);
        assert_eq!(ctx.rounds, 3);
        assert_eq!(ctx.tool_calls, 5);
        assert_eq!(ctx.total_input_tokens, 1000);
        assert_eq!(ctx.total_output_tokens, 500);

        // Accumulation
        ctx.record_completion(2, 3, 2000, 1000);
        assert_eq!(ctx.rounds, 5);
        assert_eq!(ctx.tool_calls, 8);
        assert_eq!(ctx.total_input_tokens, 3000);
        assert_eq!(ctx.total_output_tokens, 1500);
    }
}
