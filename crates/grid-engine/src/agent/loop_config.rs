use std::path::PathBuf;
use std::sync::Arc;

use grid_types::skill::SkillDefinition;
use grid_types::{SandboxId, SessionId, ToolContext, UserId};

use crate::context::{
    CompactionPipeline, CompactionPipelineConfig, ContextBudgetManager, ContextPruner,
};
use crate::event::TelemetryBus;
use crate::hooks::HookRegistry;
use crate::memory::round_memory::RoundMemoryConfig;
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::security::{AiDefence, CanaryGuardLayer, PermissionEngine, SafetyPipeline};
use crate::tools::approval::{ApprovalGate, ApprovalManager};
use crate::tools::recorder::ToolExecutionRecorder;
use crate::tools::ToolRegistry;

use super::autonomous::{AutonomousConfig, AutonomousControl};
use super::config::AgentConfig;
use super::entry::AgentManifest;
use super::events::AgentLoopResult;
use super::estop::EmergencyStop;
use super::self_repair::SelfRepairManager;
use super::loop_guard::LoopGuard;
use super::stop_hooks::StopHook;
use super::subagent::SubAgentManager;
use super::CancellationToken;

/// Completion callback type — invoked after agent loop ends with a result (Phase AZ).
///
/// Allows agent definitions to register post-completion logic (e.g. cleanup,
/// metrics, chaining). Receives the final `AgentLoopResult` by reference.
pub type CompletionCallback = Arc<dyn Fn(&AgentLoopResult) + Send + Sync>;

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
    /// LLM-based compaction pipeline for summarizing old messages on PTL (AP-T6).
    pub compaction_pipeline: Option<Arc<CompactionPipeline>>,
    /// ADR-V2-018 (S3.T1): optional compaction config carried independently of
    /// the pipeline arc. The harness consults this for the proactive trigger
    /// threshold and `reactive_only` flag. When `None`, the harness falls back
    /// to `compaction_pipeline.config()` if a pipeline is wired, otherwise to
    /// `CompactionPipelineConfig::default()`.
    /// TODO(ADR-V2-018): wire from YAML — currently agent runtimes call
    /// `AgentLoopConfigBuilder::compaction_pipeline` directly.
    pub compaction_config: Option<CompactionPipelineConfig>,

    // === Optional components ===
    /// Tool execution recorder for observability.
    pub recorder: Option<Arc<ToolExecutionRecorder>>,
    /// Event bus for pub/sub.
    pub event_bus: Option<Arc<TelemetryBus>>,
    /// Hook registry for lifecycle hooks.
    pub hook_registry: Option<Arc<HookRegistry>>,
    /// AI defence (injection detection, PII detection).
    pub defence: Option<Arc<AiDefence>>,
    /// Composable safety pipeline (injection, PII, canary, credentials).
    pub safety_pipeline: Option<Arc<SafetyPipeline>>,
    /// Canary token to inject into system prompt for exfiltration detection.
    pub canary_token: Option<String>,
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

    // === Skill support ===
    /// Available skill definitions for the skill index in the system prompt.
    pub skills: Option<Vec<SkillDefinition>>,
    /// Currently active skill whose body is injected into the system prompt.
    pub active_skill: Option<SkillDefinition>,

    // === Sub-agent support (D4) ===
    /// Sub-agent manager for recursive agent spawning.
    pub subagent_manager: Option<Arc<SubAgentManager>>,

    // === Tool approval (T3) ===
    /// Approval manager for three-level tool approval enforcement.
    pub approval_manager: Option<Arc<ApprovalManager>>,
    /// Shared approval gate for pending human approval requests.
    pub approval_gate: Option<ApprovalGate>,

    // === Collaboration (D6) ===
    /// Optional collaboration context for multi-agent mode.
    pub collaboration_context: Option<Arc<super::collaboration::context::CollaborationContext>>,

    // === Emergency Stop (W7-T4) ===
    /// Optional emergency stop mechanism for cooperative halt.
    pub estop: Option<EmergencyStop>,

    // === Self-Repair (W7-T1) ===
    /// Optional self-repair manager for detecting and recovering from stuck agents.
    pub self_repair: Option<SelfRepairManager>,

    // === D87 Fix 2 (L2b) ===
    /// Whether the current provider × model combination is known to honor
    /// `tool_choice=Required`. Decided at AgentRuntime layer by querying the
    /// `CapabilityStore`. When `false`, the harness skips the D87
    /// continuation injection — the workflow ends normally on text-only
    /// turns rather than triggering a doomed retry.
    pub tool_choice_supported: bool,

    // === D87 L1 metadata (skill workflow declaration) ===
    /// Optional list of tools the active skill declares as required
    /// (`workflow.required_tools` in the skill frontmatter). When set, the
    /// harness uses it to decide:
    ///   * Whether continuation is needed at all (skip if all listed tools
    ///     have already been called)
    ///   * Which specific tool to point the LLM at next via
    ///     `tool_choice=Specific(...)` — much stronger signal than the
    ///     generic `tool_choice=Required`
    pub required_tools: Option<Vec<String>>,

    // === Canary Guard (W10) ===
    /// Optional canary guard layer for per-turn canary rotation.
    pub canary_guard: Option<CanaryGuardLayer>,

    // === Permission Engine (Phase AP-T8) ===
    /// 6-layer permission engine for tool call authorization.
    /// Evaluated before ApprovalManager; Deny/Allow/Ask override tool defaults.
    pub permission_engine: Option<Arc<PermissionEngine>>,

    // === Session Summary Store (Phase AG) ===
    /// Session summary store for cross-session context injection.
    pub session_summary_store: Option<Arc<crate::memory::SessionSummaryStore>>,

    // === Cost Tracking (Phase AP-T15) ===
    /// Cumulative per-model cost tracker with cache token granularity.
    pub cost_tracker: Option<crate::metering::cost_tracker::CostTracker>,

    // === Autonomous Mode (Phase AP-T14) ===
    /// Autonomous running mode configuration.
    pub autonomous: Option<AutonomousConfig>,

    // === Autonomous Control Channels (Phase AU-G1) ===
    /// Control channels for real-time user intervention during autonomous sleep.
    /// Enables pause/resume signals and user message injection via `tokio::select!`.
    pub autonomous_control: Option<AutonomousControl>,

    // === Per-Round Memory Extraction (Phase AP-D5) ===
    /// Configuration for incremental per-round memory extraction.
    /// When enabled, memories are captured after each tool-call round
    /// instead of only at session end.
    pub round_memory_config: Option<RoundMemoryConfig>,

    // === Interaction Gate (Phase AQ-T1) ===
    /// Async interaction gate for agent-to-user communication.
    pub interaction_gate: Option<Arc<crate::tools::interaction::InteractionGate>>,

    // === Blob Store (Phase AQ-T3) ===
    /// Content-addressed blob store for externalizing large tool outputs.
    pub blob_store: Option<Arc<crate::storage::BlobStore>>,

    // === Transcript Writer (Phase AR-T2) ===
    /// Append-only JSONL transcript for session audit trail.
    pub transcript_writer: Option<Arc<crate::session::TranscriptWriter>>,

    // === Working Directory (Phase AS) ===
    /// Working directory for bootstrap file discovery (CLAUDE.md etc.) and git context.
    pub working_dir: Option<PathBuf>,

    // === Git Context (Phase AS) ===
    /// Pre-collected git context (branch, status, recent commits) for system prompt injection.
    pub git_context: Option<GitContext>,

    // === Completion Callback (Phase AZ) ===
    /// Optional callback invoked when the agent loop completes with a result.
    /// Enables agent definitions to register post-completion logic.
    pub on_completion: Option<CompletionCallback>,

    // === Stop Hooks (S3.T4) ===
    /// Hooks that run when the agent loop reaches its natural termination
    /// (`EndTurn` with no tool uses pending). Each hook can return either
    /// `Noop` (let termination proceed) or `InjectAndContinue(messages)`
    /// to push messages and re-enter the loop. Bounded by
    /// [`super::stop_hooks::MAX_STOP_HOOK_INJECTIONS`] per loop invocation.
    /// Empty by default — the harness skips dispatch entirely if empty.
    pub stop_hooks: Vec<Arc<dyn StopHook>>,
}

/// Pre-collected git information for system prompt injection.
#[derive(Debug, Clone)]
pub struct GitContext {
    pub branch: String,
    pub status: String,
    pub recent_commits: String,
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
            compaction_pipeline: None,
            compaction_config: None,
            recorder: None,
            event_bus: None,
            hook_registry: None,
            defence: None,
            safety_pipeline: None,
            canary_token: None,
            manifest: None,
            interceptor: None,
            session_id: SessionId::default(),
            user_id: UserId::default(),
            sandbox_id: SandboxId::default(),
            tool_ctx: None,
            cancel_token: CancellationToken::new(),
            agent_config: AgentConfig::default(),
            skills: None,
            active_skill: None,
            subagent_manager: None,
            approval_manager: None,
            approval_gate: None,
            collaboration_context: None,
            estop: None,
            self_repair: None,
            tool_choice_supported: false,
            required_tools: None,
            canary_guard: None,
            permission_engine: None,
            session_summary_store: None,
            cost_tracker: None,
            autonomous: None,
            autonomous_control: None,
            round_memory_config: None,
            interaction_gate: None,
            blob_store: None,
            transcript_writer: None,
            working_dir: None,
            git_context: None,
            on_completion: None,
            stop_hooks: Vec::new(),
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
            .field("has_estop", &self.estop.is_some())
            .field("has_self_repair", &self.self_repair.is_some())
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

    pub fn compaction_pipeline(mut self, v: Arc<CompactionPipeline>) -> Self {
        self.config.compaction_pipeline = Some(v);
        self
    }

    /// ADR-V2-018: set the compaction config the harness will consult for
    /// proactive trigger thresholds. When omitted, the harness derives the
    /// config from `compaction_pipeline.config()` if a pipeline is wired.
    pub fn compaction_config(mut self, v: CompactionPipelineConfig) -> Self {
        self.config.compaction_config = Some(v);
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

    pub fn safety_pipeline(mut self, v: Arc<SafetyPipeline>) -> Self {
        self.config.safety_pipeline = Some(v);
        self
    }

    pub fn canary_token(mut self, v: String) -> Self {
        self.config.canary_token = Some(v);
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

    pub fn skills(mut self, v: Vec<SkillDefinition>) -> Self {
        self.config.skills = Some(v);
        self
    }

    pub fn active_skill(mut self, v: SkillDefinition) -> Self {
        self.config.active_skill = Some(v);
        self
    }

    pub fn subagent_manager(mut self, v: Arc<SubAgentManager>) -> Self {
        self.config.subagent_manager = Some(v);
        self
    }

    pub fn approval_manager(mut self, v: Arc<ApprovalManager>) -> Self {
        self.config.approval_manager = Some(v);
        self
    }

    pub fn approval_gate(mut self, v: ApprovalGate) -> Self {
        self.config.approval_gate = Some(v);
        self
    }

    pub fn collaboration_context(
        mut self,
        v: Arc<super::collaboration::context::CollaborationContext>,
    ) -> Self {
        self.config.collaboration_context = Some(v);
        self
    }

    pub fn estop(mut self, v: EmergencyStop) -> Self {
        self.config.estop = Some(v);
        self
    }

    pub fn self_repair(mut self, v: SelfRepairManager) -> Self {
        self.config.self_repair = Some(v);
        self
    }

    /// D87 Fix 2 (L2b): mark whether the current provider × model can honor
    /// `tool_choice=Required`. Set by `AgentRuntime` after consulting its
    /// `CapabilityStore`. When `false`, the harness skips the workflow
    /// continuation injection.
    pub fn tool_choice_supported(mut self, supported: bool) -> Self {
        self.config.tool_choice_supported = supported;
        self
    }

    /// D87 L1 metadata: declare the tools the active skill requires.
    /// The harness uses this to point the LLM at the next missing tool
    /// (via `tool_choice=Specific`) and to short-circuit continuation when
    /// every required tool has already been called.
    pub fn required_tools(mut self, names: Vec<String>) -> Self {
        self.config.required_tools = Some(names);
        self
    }

    pub fn with_canary_guard(mut self, cg: CanaryGuardLayer) -> Self {
        self.config.canary_guard = Some(cg);
        self
    }

    pub fn permission_engine(mut self, v: Arc<PermissionEngine>) -> Self {
        self.config.permission_engine = Some(v);
        self
    }

    pub fn cost_tracker(mut self, v: crate::metering::cost_tracker::CostTracker) -> Self {
        self.config.cost_tracker = Some(v);
        self
    }

    pub fn autonomous(mut self, v: AutonomousConfig) -> Self {
        self.config.autonomous = Some(v);
        self
    }

    pub fn autonomous_control(mut self, v: AutonomousControl) -> Self {
        self.config.autonomous_control = Some(v);
        self
    }

    pub fn round_memory_config(mut self, v: RoundMemoryConfig) -> Self {
        self.config.round_memory_config = Some(v);
        self
    }

    pub fn interaction_gate(mut self, v: Arc<crate::tools::interaction::InteractionGate>) -> Self {
        self.config.interaction_gate = Some(v);
        self
    }

    pub fn blob_store(mut self, v: Arc<crate::storage::BlobStore>) -> Self {
        self.config.blob_store = Some(v);
        self
    }

    pub fn transcript_writer(mut self, v: Arc<crate::session::TranscriptWriter>) -> Self {
        self.config.transcript_writer = Some(v);
        self
    }

    pub fn working_dir(mut self, v: PathBuf) -> Self {
        self.config.working_dir = Some(v);
        self
    }

    pub fn git_context(mut self, v: GitContext) -> Self {
        self.config.git_context = Some(v);
        self
    }

    /// S3.T4: register a stop hook fired at natural loop termination.
    pub fn stop_hook(mut self, hook: Arc<dyn StopHook>) -> Self {
        self.config.stop_hooks.push(hook);
        self
    }

    /// S3.T4: replace the entire stop-hook list (chiefly for tests).
    pub fn stop_hooks(mut self, hooks: Vec<Arc<dyn StopHook>>) -> Self {
        self.config.stop_hooks = hooks;
        self
    }

    pub fn build(self) -> AgentLoopConfig {
        self.config
    }
}
