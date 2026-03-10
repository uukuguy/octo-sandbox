use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::info;

use octo_types::{ChatMessage, SandboxId, SessionId, ToolContext, UserId};

use crate::context::ContextBudgetManager;
use crate::context::ContextPruner;
use crate::hooks::HookRegistry;
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

use super::config::AgentConfig;
use super::entry::AgentManifest;
use super::events::AgentEvent;
use super::harness::run_agent_loop;
use super::loop_config::AgentLoopConfig;
use super::CancellationToken;

/// Backward-compatible wrapper around [`run_agent_loop()`].
///
/// New code should use [`run_agent_loop()`] directly with [`AgentLoopConfig`].
/// This struct exists solely so that [`AgentExecutor`] (and any other legacy
/// consumer) can continue to work without modification.
pub struct AgentLoop {
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    model: String,
    max_tokens: u32,
    budget: ContextBudgetManager,
    pruner: ContextPruner,
    recorder: Option<Arc<crate::tools::recorder::ToolExecutionRecorder>>,
    loop_guard: super::loop_guard::LoopGuard,
    event_bus: Option<Arc<crate::event::TelemetryBus>>,
    hook_registry: Option<Arc<HookRegistry>>,
    config: AgentConfig,
    /// Zone A: Agent manifest containing role/goal/backstory/system_prompt
    manifest: Option<AgentManifest>,
    /// Zone A: override the entire system prompt (deprecated, use manifest instead).
    /// Kept for backward compatibility.
    #[deprecated(since = "0.1.0", note = "Use with_manifest() instead")]
    system_prompt_override: Option<String>,
    /// AIDefence: injection + PII detection on user input (optional, disabled by default).
    defence: Option<Arc<crate::security::AiDefence>>,
}

impl AgentLoop {
    /// Create new AgentLoop - model must be set via with_model() before use
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
    ) -> Self {
        Self {
            provider,
            tools,
            memory,
            memory_store: None,
            model: "".into(), // Must call with_model() before running
            max_tokens: 4096,
            budget: ContextBudgetManager::default(),
            pruner: ContextPruner::new(),
            recorder: None,
            loop_guard: super::loop_guard::LoopGuard::new(),
            event_bus: None,
            hook_registry: None,
            config: AgentConfig::default(),
            manifest: None,
            #[allow(deprecated)]
            system_prompt_override: None,
            defence: None,
        }
    }

    pub fn with_defence(mut self, defence: Arc<crate::security::AiDefence>) -> Self {
        self.defence = Some(defence);
        self
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<crate::event::TelemetryBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Zone A: set the agent manifest (role/goal/backstory/system_prompt).
    ///
    /// The manifest is used by SystemPromptBuilder to construct Zone A:
    /// - system_prompt: Full override (highest priority)
    /// - role/goal/backstory: CrewAI pattern (second priority)
    pub fn with_manifest(mut self, manifest: AgentManifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Zone A: override the system prompt with a custom string (e.g. from AgentManifest).
    /// When set, the default SystemPromptBuilder is bypassed entirely.
    #[allow(deprecated)]
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt_override = Some(prompt);
        self
    }

    pub fn with_recorder(
        mut self,
        recorder: Arc<crate::tools::recorder::ToolExecutionRecorder>,
    ) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// Run the agent loop by delegating to [`run_agent_loop()`].
    ///
    /// Builds an [`AgentLoopConfig`] from `self` fields and method parameters,
    /// spawns the harness, and bridges the returned event stream to the
    /// broadcast sender expected by legacy consumers.
    #[deprecated(since = "0.2.0", note = "Use harness::run_agent_loop() directly")]
    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &mut self,
        session_id: &SessionId,
        user_id: &UserId,
        sandbox_id: &SandboxId,
        messages: &mut Vec<ChatMessage>,
        tx: broadcast::Sender<AgentEvent>,
        tool_ctx: ToolContext,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> Result<()> {
        // Panic if model not set - must call with_model() first
        assert!(
            !self.model.is_empty(),
            "Model not set: call with_model() before run()"
        );

        info!(
            session = %session_id,
            "AgentLoop starting (delegating to harness), {} messages in history",
            messages.len()
        );

        // Build a CancellationToken and bridge the legacy AtomicBool flag
        let cancel_token = CancellationToken::new();
        if let Some(ref flag) = cancel_flag {
            let flag_clone = flag.clone();
            let token_clone = cancel_token.clone();
            tokio::spawn(async move {
                loop {
                    if flag_clone.load(Ordering::Relaxed) {
                        token_clone.cancel();
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            });
        }

        // Resolve manifest: if system_prompt_override is set, wrap it into a manifest
        #[allow(deprecated)]
        let manifest = if let Some(ref prompt_override) = self.system_prompt_override {
            Some(AgentManifest {
                name: String::new(),
                tags: Vec::new(),
                role: None,
                goal: None,
                backstory: None,
                system_prompt: Some(prompt_override.clone()),
                model: None,
                tool_filter: Vec::new(),
                config: AgentConfig::default(),
                max_concurrent_tasks: 0,
                priority: None,
            })
        } else {
            self.manifest.clone()
        };

        // Build AgentLoopConfig from self fields
        let loop_config = AgentLoopConfig {
            max_iterations: if self.config.max_rounds == 0 {
                u32::MAX
            } else {
                self.config.max_rounds
            },
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            provider: Some(self.provider.clone()),
            tools: Some(self.tools.clone()),
            memory: Some(self.memory.clone()),
            memory_store: self.memory_store.clone(),
            budget: Some(self.budget.clone()),
            pruner: Some(self.pruner.clone()),
            loop_guard: Some(self.loop_guard.clone()),
            recorder: self.recorder.clone(),
            event_bus: self.event_bus.clone(),
            hook_registry: self.hook_registry.clone(),
            defence: self.defence.clone(),
            manifest,
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            sandbox_id: sandbox_id.clone(),
            tool_ctx: Some(tool_ctx),
            cancel_token,
            agent_config: self.config.clone(),
            ..AgentLoopConfig::default()
        };

        // Call the harness and consume the event stream
        let mut stream = run_agent_loop(loop_config, messages.clone());

        while let Some(event) = stream.next().await {
            // Capture final_messages from the Completed event to update caller's messages
            if let AgentEvent::Completed(ref result) = event {
                if !result.final_messages.is_empty() {
                    *messages = result.final_messages.clone();
                }
            }

            // Forward every event to the broadcast sender
            let is_done = matches!(event, AgentEvent::Done);
            let _ = tx.send(event);

            if is_done {
                break;
            }
        }

        Ok(())
    }
}

/// Handle a provider error WITHOUT persisting it to conversation history.
/// Returns true if the error is retryable (caller should continue loop).
///
/// Nanobot principle: provider errors are infrastructure failures, not conversation events.
/// Error responses are sent via AgentEvent::Error but never appended to the messages vector.
///
/// The function signature takes `&[ChatMessage]` (immutable slice) rather than
/// `&mut Vec<ChatMessage>` -- this enforces the non-persistence guarantee at the type level.
pub fn handle_provider_error_non_persistent(
    error_msg: &str,
    _messages: &[ChatMessage], // read-only reference -- never mutated
) -> bool {
    use crate::providers::LlmErrorKind;
    let kind = LlmErrorKind::classify_from_str(&error_msg.to_lowercase());
    kind.is_retryable()
}
