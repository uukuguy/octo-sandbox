use std::sync::Arc;

use octo_types::{SandboxId, SessionId, ToolContext, UserId};

use crate::context::{ContextBudgetManager, ContextPruner};
use crate::event::TelemetryBus;
use crate::hooks::HookRegistry;
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::security::AiDefence;
use crate::tools::recorder::ToolExecutionRecorder;
use crate::tools::ToolRegistry;

use super::config::AgentConfig;
use super::entry::AgentManifest;
use super::loop_guard::LoopGuard;
use super::subagent::SubAgentManager;
use super::CancellationToken;

/// Agent Loop configuration — serves as the complete dependency injection container
/// for `run_agent_loop()`. Inspired by IronClaw AgentDeps + ZeroClaw run_agent_loop().
pub struct AgentLoopConfig {
    // === Control parameters ===
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

    // === Core dependencies (required for harness) ===
    /// LLM provider (Anthropic, OpenAI, etc.)
    pub provider: Option<Arc<dyn Provider>>,
    /// Tool registry with all available tools.
    pub tools: Option<Arc<ToolRegistry>>,
    /// Working memory for the current conversation.
    pub memory: Option<Arc<dyn WorkingMemory>>,
    /// Persistent memory store (optional).
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    /// Model name (e.g. "claude-sonnet-4-20250514").
    pub model: String,
    /// Max tokens for LLM response.
    pub max_tokens: u32,

    // === Context management ===
    /// Token budget manager for context window.
    pub budget: Option<ContextBudgetManager>,
    /// Context pruner for trimming old messages.
    pub pruner: Option<ContextPruner>,
    /// Loop guard to prevent infinite loops and detect repetition.
    pub loop_guard: Option<LoopGuard>,

    // === Optional components ===
    /// Tool execution recorder for observability.
    pub recorder: Option<Arc<ToolExecutionRecorder>>,
    /// Event bus for pub/sub.
    pub event_bus: Option<Arc<TelemetryBus>>,
    /// Hook registry for lifecycle hooks.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// AI defence (injection detection, PII detection).
    pub defence: Option<Arc<AiDefence>>,
    /// Agent manifest (role/goal/backstory/system_prompt).
    pub manifest: Option<AgentManifest>,
    /// Tool call interceptor for skill-based tool filtering.
    pub interceptor: Option<crate::tools::interceptor::ToolCallInterceptor>,

    // === Session context ===
    /// Session ID for this agent turn.
    pub session_id: SessionId,
    /// User ID.
    pub user_id: UserId,
    /// Sandbox ID.
    pub sandbox_id: SandboxId,
    /// Tool execution context.
    pub tool_ctx: Option<ToolContext>,
    /// Cancellation token for cooperative cancellation.
    pub cancel_token: CancellationToken,

    // === Agent behavior ===
    /// Agent-level behavior configuration.
    pub agent_config: AgentConfig,

    // === Sub-agent support (D4) ===
    /// Sub-agent manager for recursive agent spawning.
    pub subagent_manager: Option<Arc<SubAgentManager>>,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 30,
            max_concurrent_tools: 8,
            tool_timeout_secs: 120,
            force_text_at_last: true,
            max_tokens_continuation: 3,
            provider: None,
            tools: None,
            memory: None,
            memory_store: None,
            model: String::new(),
            max_tokens: 4096,
            budget: None,
            pruner: None,
            loop_guard: None,
            recorder: None,
            event_bus: None,
            hook_registry: None,
            defence: None,
            manifest: None,
            interceptor: None,
            session_id: SessionId::default(),
            user_id: UserId::default(),
            sandbox_id: SandboxId::default(),
            tool_ctx: None,
            cancel_token: CancellationToken::new(),
            agent_config: AgentConfig::default(),
            subagent_manager: None,
        }
    }
}

impl std::fmt::Debug for AgentLoopConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoopConfig")
            .field("max_iterations", &self.max_iterations)
            .field("max_concurrent_tools", &self.max_concurrent_tools)
            .field("tool_timeout_secs", &self.tool_timeout_secs)
            .field("force_text_at_last", &self.force_text_at_last)
            .field("max_tokens_continuation", &self.max_tokens_continuation)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("has_provider", &self.provider.is_some())
            .field("has_tools", &self.tools.is_some())
            .field("has_memory", &self.memory.is_some())
            .field("session_id", &self.session_id)
            .finish()
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

    pub fn provider(mut self, v: Arc<dyn Provider>) -> Self {
        self.config.provider = Some(v);
        self
    }

    pub fn tools(mut self, v: Arc<ToolRegistry>) -> Self {
        self.config.tools = Some(v);
        self
    }

    pub fn memory(mut self, v: Arc<dyn WorkingMemory>) -> Self {
        self.config.memory = Some(v);
        self
    }

    pub fn memory_store(mut self, v: Arc<dyn MemoryStore>) -> Self {
        self.config.memory_store = Some(v);
        self
    }

    pub fn model(mut self, v: String) -> Self {
        self.config.model = v;
        self
    }

    pub fn max_tokens(mut self, v: u32) -> Self {
        self.config.max_tokens = v;
        self
    }

    pub fn budget(mut self, v: ContextBudgetManager) -> Self {
        self.config.budget = Some(v);
        self
    }

    pub fn pruner(mut self, v: ContextPruner) -> Self {
        self.config.pruner = Some(v);
        self
    }

    pub fn loop_guard(mut self, v: LoopGuard) -> Self {
        self.config.loop_guard = Some(v);
        self
    }

    pub fn recorder(mut self, v: Arc<ToolExecutionRecorder>) -> Self {
        self.config.recorder = Some(v);
        self
    }

    pub fn event_bus(mut self, v: Arc<TelemetryBus>) -> Self {
        self.config.event_bus = Some(v);
        self
    }

    pub fn hook_registry(mut self, v: Arc<HookRegistry>) -> Self {
        self.config.hook_registry = Some(v);
        self
    }

    pub fn defence(mut self, v: Arc<AiDefence>) -> Self {
        self.config.defence = Some(v);
        self
    }

    pub fn manifest(mut self, v: AgentManifest) -> Self {
        self.config.manifest = Some(v);
        self
    }

    pub fn interceptor(mut self, v: crate::tools::interceptor::ToolCallInterceptor) -> Self {
        self.config.interceptor = Some(v);
        self
    }

    pub fn session_id(mut self, v: SessionId) -> Self {
        self.config.session_id = v;
        self
    }

    pub fn user_id(mut self, v: UserId) -> Self {
        self.config.user_id = v;
        self
    }

    pub fn sandbox_id(mut self, v: SandboxId) -> Self {
        self.config.sandbox_id = v;
        self
    }

    pub fn tool_ctx(mut self, v: ToolContext) -> Self {
        self.config.tool_ctx = Some(v);
        self
    }

    pub fn cancel_token(mut self, v: CancellationToken) -> Self {
        self.config.cancel_token = v;
        self
    }

    pub fn agent_config(mut self, v: AgentConfig) -> Self {
        self.config.agent_config = v;
        self
    }

    pub fn subagent_manager(mut self, v: Arc<SubAgentManager>) -> Self {
        self.config.subagent_manager = Some(v);
        self
    }

    pub fn build(self) -> AgentLoopConfig {
        self.config
    }
}
