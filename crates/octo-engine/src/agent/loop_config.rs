/// Agent Loop configuration — replaces run()'s control parameters.
/// Inspired by IronClaw AgentDeps + ZeroClaw run_agent_loop().
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// Maximum number of LLM iterations per turn.
    pub max_iterations: u32,
    /// Maximum number of tools to execute concurrently.
    pub max_concurrent_tools: usize,
    /// Timeout in seconds for individual tool execution.
    pub tool_timeout_secs: u64,
    /// Whether to force text output on the last iteration.
    pub force_text_at_last: bool,
    /// Maximum number of continuation requests when output is truncated.
    pub max_tokens_continuation: u32,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 30,
            max_concurrent_tools: 8,
            tool_timeout_secs: 120,
            force_text_at_last: true,
            max_tokens_continuation: 3,
        }
    }
}

impl AgentLoopConfig {
    pub fn builder() -> AgentLoopConfigBuilder {
        AgentLoopConfigBuilder::default()
    }
}

#[derive(Default)]
pub struct AgentLoopConfigBuilder {
    config: AgentLoopConfig,
}

impl AgentLoopConfigBuilder {
    pub fn max_iterations(mut self, v: u32) -> Self {
        self.config.max_iterations = v;
        self
    }

    pub fn max_concurrent_tools(mut self, v: usize) -> Self {
        self.config.max_concurrent_tools = v;
        self
    }

    pub fn tool_timeout_secs(mut self, v: u64) -> Self {
        self.config.tool_timeout_secs = v;
        self
    }

    pub fn force_text_at_last(mut self, v: bool) -> Self {
        self.config.force_text_at_last = v;
        self
    }

    pub fn max_tokens_continuation(mut self, v: u32) -> Self {
        self.config.max_tokens_continuation = v;
        self
    }

    pub fn build(self) -> AgentLoopConfig {
        self.config
    }
}
