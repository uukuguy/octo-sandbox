use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, SandboxId, SessionId, StopReason,
    StreamEvent, ToolContext, UserId,
};

use crate::context::{
    ContextBudgetManager,
    ContextPruner,
    DegradationLevel,
    MemoryFlusher,
    NewSystemPromptBuilder as SystemPromptBuilder, // Zone A builder
};
use crate::hooks::{HookContext, HookPoint, HookRegistry};
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::{LlmErrorKind, Provider, RetryPolicy};
use crate::tools::ToolRegistry;

use super::config::AgentConfig;
use super::entry::AgentManifest;
use super::parallel::execute_parallel;
use super::CancellationToken;

const MAX_ROUNDS: u32 = 30;
const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

/// Events sent from AgentLoop to consumers (WebSocket handler)
#[derive(Debug, Clone)]
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
}

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
    event_bus: Option<Arc<crate::event::EventBus>>,
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

    pub fn with_event_bus(mut self, bus: Arc<crate::event::EventBus>) -> Self {
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
            "AgentLoop starting, {} messages in history",
            messages.len()
        );

        // Zone A: static system prompt (agent identity + capabilities)
        //
        // Priority:
        // 1. system_prompt_override (deprecated, for backward compatibility)
        // 2. manifest with role/goal/backstory/system_prompt
        // 3. default SystemPromptBuilder
        #[allow(deprecated)]
        let system_prompt = if let Some(ref r#override) = self.system_prompt_override {
            r#override.clone()
        } else if let Some(ref manifest) = self.manifest {
            SystemPromptBuilder::new()
                .with_manifest(manifest.clone())
                .build()
        } else {
            SystemPromptBuilder::new().build()
        };

        debug!("System prompt length: {} chars", system_prompt.len());

        // Zone B: dynamic context injected as first human message
        // Working memory (UserProfile, TaskContext, AutoExtracted, Custom blocks)
        // is compiled into a <context> XML block and prepended to the conversation.
        let memory_xml = self
            .memory
            .compile(user_id, sandbox_id)
            .await
            .unwrap_or_default();

        if !memory_xml.is_empty() {
            let zone_b = ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: memory_xml.clone(),
                }],
            };
            // Replace existing Zone B injection (if any) or prepend a new one.
            let first_is_context = messages
                .first()
                .and_then(|m| m.content.first())
                .map(|b| matches!(b, ContentBlock::Text { text } if text.starts_with("<context>")))
                .unwrap_or(false);
            if first_is_context {
                messages[0] = zone_b;
            } else {
                messages.insert(0, zone_b);
            }
        }

        debug!("Zone B injected: working memory {} chars", memory_xml.len());

        let tool_specs = self.tools.specs();

        // Determine max rounds: 0 means infinite
        let max_rounds = if self.config.max_rounds == 0 {
            u32::MAX
        } else {
            self.config.max_rounds
        };

        // SessionStart hook
        if let Some(ref hooks) = self.hook_registry {
            let ctx = HookContext::new().with_session(session_id.as_str());
            hooks.execute(HookPoint::SessionStart, &ctx).await;
        }

        for round in 0..max_rounds {
            debug!(round, "Agent round starting");

            // PreTask hook (fires once before the first round)
            if round == 0 {
                if let Some(ref hooks) = self.hook_registry {
                    let ctx = HookContext::new()
                        .with_session(session_id.as_str())
                        .with_turn(round);
                    if let crate::hooks::HookAction::Abort(reason) =
                        hooks.execute(HookPoint::PreTask, &ctx).await
                    {
                        let _ = tx.send(AgentEvent::Error {
                            message: reason.clone(),
                        });
                        let _ = tx.send(AgentEvent::Done);
                        if let Some(ref hooks) = self.hook_registry {
                            let ctx = HookContext::new().with_session(session_id.as_str());
                            hooks.execute(HookPoint::SessionEnd, &ctx).await;
                        }
                        return Err(anyhow::anyhow!("PreTask hook aborted: {}", reason));
                    }
                }
            }

            // 发布 LoopTurnStarted 事件（用于指标统计）
            if let Some(ref bus) = self.event_bus {
                bus.publish(crate::event::OctoEvent::LoopTurnStarted {
                    session_id: session_id.as_str().to_string(),
                    turn: round,
                })
                .await;
            }

            // LoopTurnStart hook
            if let Some(ref hooks) = self.hook_registry {
                let ctx = HookContext::new()
                    .with_session(session_id.as_str())
                    .with_turn(round);
                if let crate::hooks::HookAction::Abort(reason) =
                    hooks.execute(HookPoint::LoopTurnStart, &ctx).await
                {
                    let _ = tx.send(AgentEvent::Error {
                        message: reason.clone(),
                    });
                    let _ = tx.send(AgentEvent::Done);
                    if let Some(ref hooks) = self.hook_registry {
                        let ctx = HookContext::new().with_session(session_id.as_str());
                        hooks.execute(HookPoint::SessionEnd, &ctx).await;
                    }
                    return Err(anyhow::anyhow!("LoopTurnStart hook aborted: {}", reason));
                }
            }
            let turn_start = std::time::Instant::now();

            // Check for cancellation
            if let Some(flag) = &cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    info!(session = %session_id, "Agent loop cancelled");
                    break;
                }
            }

            // Apply context pruning based on budget
            let level =
                self.budget
                    .compute_degradation_level(&system_prompt, messages, &tool_specs);
            if level != DegradationLevel::None {
                debug!(?level, "Applying context degradation");

                // At OverflowCompaction level: flush facts before pruning to prevent info loss
                if level >= DegradationLevel::OverflowCompaction
                    && level != DegradationLevel::FinalError
                {
                    let boundary = ContextPruner::find_compaction_boundary(messages, 20_000);
                    if boundary > 0 {
                        let _ = MemoryFlusher::flush(
                            messages,
                            boundary,
                            &*self.provider,
                            &*self.memory,
                            self.memory_store.as_deref(),
                            &self.model,
                            user_id.as_str(),
                        )
                        .await;
                    }
                }

                self.pruner.apply(messages, level);

                // ContextDegraded hook
                if let Some(ref hooks) = self.hook_registry {
                    let ctx = HookContext::new()
                        .with_session(session_id.as_str())
                        .with_turn(round)
                        .with_degradation(format!("{:?}", level));
                    hooks.execute(HookPoint::ContextDegraded, &ctx).await;
                }
            }

            // AIDefence: check user input before the first LLM call.
            if round == 0 {
                if let Some(ref defence) = self.defence {
                    let user_text: Option<String> = messages
                        .iter()
                        .rev()
                        .find(|m| m.role == MessageRole::User)
                        .and_then(|m| {
                            m.content.iter().find_map(|b| {
                                if let ContentBlock::Text { text } = b {
                                    Some(text.clone())
                                } else {
                                    None
                                }
                            })
                        });
                    if let Some(ref text) = user_text {
                        if let Err(violation) = defence.check_input(text) {
                            tracing::warn!(violation = %violation, "AIDefence blocked input");
                            let _ = tx.send(AgentEvent::Error {
                                message: format!("Security check failed: {violation}"),
                            });
                            let _ = tx.send(AgentEvent::Done);
                            return Ok(());
                        }
                    }
                }
            }

            let request = CompletionRequest {
                model: self.model.clone(),
                system: Some(system_prompt.clone()),
                messages: messages.clone(),
                max_tokens: self.max_tokens,
                temperature: None,
                tools: tool_specs.clone(),
                stream: true,
            };

            // Retry on transient API errors using LlmErrorKind classification + exponential backoff
            let retry_policy = RetryPolicy::default();
            let mut stream = None;
            let mut last_err = None;
            let mut attempt = 0u32;
            loop {
                match self.provider.stream(request.clone()).await {
                    Ok(s) => {
                        stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        if retry_policy.should_retry_str(&err_str, attempt) {
                            let delay = retry_policy.delay_for(attempt);
                            let kind = LlmErrorKind::classify_from_str(&err_str);
                            warn!(
                                "LLM stream failed (attempt {}/{}, kind={:?}), retrying in {:?}: {}",
                                attempt + 1, retry_policy.max_retries, kind, delay, e
                            );
                            tokio::time::sleep(delay).await;
                            last_err = Some(e);
                            attempt += 1;
                            continue;
                        } else {
                            let kind = LlmErrorKind::classify_from_str(&err_str);
                            tracing::error!(
                                "LLM stream failed (non-retryable, kind={:?}): {}",
                                kind,
                                e
                            );
                            last_err = Some(e);
                            break;
                        }
                    }
                }
            }

            let mut stream = match stream {
                Some(s) => s,
                None => {
                    let e = last_err.unwrap_or_else(|| anyhow::anyhow!("stream failed"));
                    let _ = tx.send(AgentEvent::Error {
                        message: e.to_string(),
                    });
                    let _ = tx.send(AgentEvent::Done);
                    return Err(e);
                }
            };

            let mut full_text = String::new();
            let mut full_thinking = String::new();
            let mut tool_uses: Vec<PendingToolUse> = Vec::new();
            let mut current_tool: Option<PendingToolUse> = None;
            let mut sent_typing = false;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(StreamEvent::MessageStart { .. }) => {}
                    Ok(StreamEvent::TextDelta { text }) => {
                        // Send typing indicator when LLM starts responding
                        if !sent_typing && self.config.enable_typing_signal {
                            let _ = tx.send(AgentEvent::Typing { state: true });
                            sent_typing = true;
                        }
                        full_text.push_str(&text);
                        let _ = tx.send(AgentEvent::TextDelta { text });
                    }
                    Ok(StreamEvent::ThinkingDelta { text }) => {
                        // Send typing indicator when thinking starts
                        if !sent_typing && self.config.enable_typing_signal {
                            let _ = tx.send(AgentEvent::Typing { state: true });
                            sent_typing = true;
                        }
                        full_thinking.push_str(&text);
                        let _ = tx.send(AgentEvent::ThinkingDelta { text });
                    }
                    Ok(StreamEvent::ToolUseStart { id, name, .. }) => {
                        current_tool = Some(PendingToolUse {
                            id,
                            name,
                            input_json: String::new(),
                        });
                    }
                    Ok(StreamEvent::ToolUseInputDelta { partial_json, .. }) => {
                        if let Some(ref mut tool) = current_tool {
                            tool.input_json.push_str(&partial_json);
                        }
                    }
                    Ok(StreamEvent::ToolUseComplete {
                        id, name, input, ..
                    }) => {
                        tool_uses.push(PendingToolUse {
                            id,
                            name,
                            input_json: input.to_string(),
                        });
                        current_tool = None;
                    }
                    Ok(StreamEvent::MessageStop { stop_reason, usage }) => {
                        debug!(
                            ?stop_reason,
                            input_tokens = usage.input_tokens,
                            output_tokens = usage.output_tokens,
                            "Message complete"
                        );

                        // Update budget with actual usage
                        self.budget
                            .update_actual_usage(usage.input_tokens, messages.len());

                        // Emit budget snapshot to frontend
                        let snapshot = self.budget.snapshot(&system_prompt, messages, &tool_specs);
                        let _ = tx.send(AgentEvent::TokenBudgetUpdate { budget: snapshot });

                        // If there's thinking but no text, the model put everything
                        // in thinking blocks (common with proxy/relay models like MiniMax).
                        // Fall back: treat thinking as the reply text.
                        if full_text.is_empty() && !full_thinking.is_empty() {
                            debug!("No text content, falling back thinking to text");
                            full_text = full_thinking.clone();
                            full_thinking.clear();
                        }

                        // Emit ThinkingComplete if there was thinking content
                        if !full_thinking.is_empty() {
                            let _ = tx.send(AgentEvent::ThinkingComplete {
                                text: full_thinking.clone(),
                            });
                            full_thinking.clear();
                        }

                        // If no tool uses, this is final response
                        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
                            // C-01: AIDefence output validation on every LLM response.
                            if !full_text.is_empty() {
                                if let Some(ref defence) = self.defence {
                                    if let Err(violation) = defence.check_output(&full_text) {
                                        tracing::warn!(violation = %violation, "AIDefence blocked output");
                                        let _ = tx.send(AgentEvent::Error {
                                            message: format!("Output security check failed: {violation}"),
                                        });
                                        let _ = tx.send(AgentEvent::Done);
                                        return Ok(());
                                    }
                                }
                            }

                            // Always append an assistant message so the conversation history
                            // stays well-formed (no two consecutive user messages).
                            messages.push(ChatMessage::assistant(if full_text.is_empty() {
                                "(no response)"
                            } else {
                                &full_text
                            }));
                            if !full_text.is_empty() {
                                let _ = tx.send(AgentEvent::TextComplete {
                                    text: full_text.clone(),
                                });
                            }
                            // Stop typing indicator
                            if sent_typing && self.config.enable_typing_signal {
                                let _ = tx.send(AgentEvent::Typing { state: false });
                            }
                            if let Some(ref hooks) = self.hook_registry {
                                let elapsed = turn_start.elapsed().as_millis() as u64;
                                let ctx = HookContext::new()
                                    .with_session(session_id.as_str())
                                    .with_turn(round)
                                    .with_result(true, elapsed);
                                hooks.execute(HookPoint::PostTask, &ctx).await;
                                hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
                                hooks.execute(HookPoint::SessionEnd, &ctx).await;
                            }
                            let _ = tx.send(AgentEvent::Done);
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {e}");
                        let _ = tx.send(AgentEvent::Error {
                            message: e.to_string(),
                        });
                        if let Some(ref hooks) = self.hook_registry {
                            let ctx = HookContext::new().with_session(session_id.as_str());
                            hooks.execute(HookPoint::SessionEnd, &ctx).await;
                        }
                        return Err(e);
                    }
                }
            }

            // If we have tool uses, execute them
            if tool_uses.is_empty() {
                // Stream ended without explicit MessageStop with tool_use.
                // C-01: AIDefence output validation.
                if !full_text.is_empty() {
                    if let Some(ref defence) = self.defence {
                        if let Err(violation) = defence.check_output(&full_text) {
                            tracing::warn!(violation = %violation, "AIDefence blocked output");
                            let _ = tx.send(AgentEvent::Error {
                                message: format!("Output security check failed: {violation}"),
                            });
                            let _ = tx.send(AgentEvent::Done);
                            return Ok(());
                        }
                    }
                }

                // Ensure an assistant message is always appended.
                messages.push(ChatMessage::assistant(if full_text.is_empty() {
                    "(no response)"
                } else {
                    &full_text
                }));
                if !full_text.is_empty() {
                    let _ = tx.send(AgentEvent::TextComplete {
                        text: full_text.clone(),
                    });
                }
                if let Some(ref hooks) = self.hook_registry {
                    let elapsed = turn_start.elapsed().as_millis() as u64;
                    let ctx = HookContext::new()
                        .with_session(session_id.as_str())
                        .with_turn(round)
                        .with_result(true, elapsed);
                    hooks.execute(HookPoint::PostTask, &ctx).await;
                    hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
                    hooks.execute(HookPoint::SessionEnd, &ctx).await;
                }
                let _ = tx.send(AgentEvent::Done);
                return Ok(());
            }

            // Build assistant message with text + tool_use blocks
            let mut assistant_content: Vec<ContentBlock> = Vec::new();
            if !full_text.is_empty() {
                assistant_content.push(ContentBlock::Text {
                    text: full_text.clone(),
                });
            }
            for tu in &tool_uses {
                let input: serde_json::Value =
                    serde_json::from_str(&tu.input_json).unwrap_or_default();
                assistant_content.push(ContentBlock::ToolUse {
                    id: tu.id.clone(),
                    name: tu.name.clone(),
                    input,
                });
            }
            messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: assistant_content,
            });

            // Execute tools and build tool result messages
            let mut tool_results: Vec<ContentBlock> = Vec::new();

            // Parse tool inputs first for loop guard check
            let parsed_tools: Vec<_> = tool_uses
                .iter()
                .map(|tu| {
                    let input: serde_json::Value =
                        serde_json::from_str(&tu.input_json).unwrap_or_default();
                    (tu, input)
                })
                .collect();

            // Loop Guard: check all tools before execution
            use super::loop_guard::LoopGuardVerdict;
            for (tu, input) in &parsed_tools {
                let verdict = self.loop_guard.check(&tu.name, input);
                match &verdict {
                    LoopGuardVerdict::Block(msg) | LoopGuardVerdict::CircuitBreak(msg) => {
                        tracing::warn!("Loop Guard blocked: {}", msg);
                        return Err(anyhow::anyhow!("Loop Guard: {}", msg));
                    }
                    LoopGuardVerdict::Warn(msg) => {
                        tracing::warn!("Loop Guard warning: {}", msg);
                    }
                    LoopGuardVerdict::Allow => {}
                }
            }

            // Send ToolStart events for all tools
            for (tu, input) in &parsed_tools {
                let _ = tx.send(AgentEvent::ToolStart {
                    tool_id: tu.id.clone(),
                    tool_name: tu.name.clone(),
                    input: input.clone(),
                });
                if let Some(ref bus) = self.event_bus {
                    bus.publish(crate::event::OctoEvent::ToolCallStarted {
                        session_id: session_id.as_str().to_string(),
                        tool_name: tu.name.clone(),
                    })
                    .await;
                }
            }

            // Execute tools - parallel or sequential based on config
            let cancellation_token = CancellationToken::new();

            let tool_outputs: Vec<_> = if self.config.enable_parallel {
                // Parallel execution
                let tools_to_run: Vec<_> = parsed_tools
                    .iter()
                    .map(|(tu, input)| (tu.name.clone(), input.clone()))
                    .collect();

                let results = execute_parallel(
                    tools_to_run,
                    &self.tools,
                    self.config.max_parallel_tools,
                    &cancellation_token,
                    &tool_ctx,
                )
                .await;

                // Map results back to tool order
                parsed_tools
                    .iter()
                    .zip(results)
                    .map(|((tu, input), (_, result))| (tu, input.clone(), result))
                    .collect()
            } else {
                // Sequential execution (original behavior)
                let mut outputs = Vec::new();
                for (tu, input) in &parsed_tools {
                    // PreToolUse hook — C-02: Block stops tool execution.
                    if let Some(ref hooks) = self.hook_registry {
                        let ctx = HookContext::new()
                            .with_session(session_id.as_str())
                            .with_tool(&tu.name, input.clone());
                        if let crate::hooks::HookAction::Block(reason) =
                            hooks.execute(HookPoint::PreToolUse, &ctx).await
                        {
                            tracing::warn!(tool = %tu.name, reason = %reason, "PreToolUse hook blocked tool");
                            let _ = tx.send(AgentEvent::Error {
                                message: format!("Tool '{}' blocked by security policy: {reason}", tu.name),
                            });
                            let _ = tx.send(AgentEvent::Done);
                            return Ok(());
                        }
                    }

                    let exec_start = std::time::Instant::now();

                    let result = if let Some(tool) = self.tools.get(&tu.name) {
                        match tool.execute(input.clone(), &tool_ctx).await {
                            Ok(r) => r,
                            Err(e) => octo_types::ToolResult::error(format!("Tool error: {e}")),
                        }
                    } else {
                        octo_types::ToolResult::error(format!("Unknown tool: {}", tu.name))
                    };

                    let exec_duration = exec_start.elapsed().as_millis() as u64;

                    // PostToolUse hook
                    if let Some(ref hooks) = self.hook_registry {
                        let mut ctx = HookContext::new()
                            .with_session(session_id.as_str())
                            .with_tool(&tu.name, input.clone())
                            .with_result(!result.is_error, exec_duration);
                        ctx.tool_result =
                            Some(serde_json::Value::String(result.output.clone()));
                        hooks.execute(HookPoint::PostToolUse, &ctx).await;
                    }

                    if let Some(ref bus) = self.event_bus {
                        bus.publish(crate::event::OctoEvent::ToolCallCompleted {
                            session_id: session_id.as_str().to_string(),
                            tool_name: tu.name.clone(),
                            duration_ms: exec_duration,
                        })
                        .await;
                    }

                    outputs.push((tu, input.clone(), result));
                }
                outputs
            };

            // Process results and send events
            for (tu, input, result) in tool_outputs {
                let _ = tx.send(AgentEvent::ToolResult {
                    tool_id: tu.id.clone(),
                    output: result.output.clone(),
                    success: !result.is_error,
                });

                // Soft-trim large tool results before injecting into messages
                let trimmed_output = maybe_trim_tool_result(&result.output);

                // C-03: AIDefence injection check on tool results (indirect prompt injection).
                // Only check injection (not PII blocking) — external tools can legitimately
                // return data containing personal information.
                if let Some(ref defence) = self.defence {
                    if let Err(violation) = defence.check_injection(&trimmed_output) {
                        tracing::warn!(
                            tool = %tu.name,
                            violation = %violation,
                            "AIDefence detected injection in tool result"
                        );
                        let _ = tx.send(AgentEvent::Error {
                            message: format!(
                                "Tool '{}' result contains injection attempt: {violation}",
                                tu.name
                            ),
                        });
                        let _ = tx.send(AgentEvent::Done);
                        return Ok(());
                    }
                }

                // Record outcome for result-aware loop detection
                if let Some(outcome_warning) =
                    self.loop_guard
                        .record_outcome(&tu.name, &input, &result.output)
                {
                    tracing::warn!("Loop Guard outcome: {}", outcome_warning);
                }

                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tu.id.clone(),
                    content: trimmed_output,
                    is_error: result.is_error,
                });
            }

            messages.push(ChatMessage {
                role: MessageRole::User,
                content: tool_results,
            });

            // LoopTurnEnd hook after tool round completes
            if let Some(ref hooks) = self.hook_registry {
                let elapsed = turn_start.elapsed().as_millis() as u64;
                let ctx = HookContext::new()
                    .with_session(session_id.as_str())
                    .with_turn(round)
                    .with_result(true, elapsed);
                hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
            }

            // Reset for next round
            full_text = String::new();
        }

        warn!("Max rounds ({MAX_ROUNDS}) exceeded");
        // Append a sentinel assistant message so history stays well-formed.
        messages.push(ChatMessage::assistant(format!(
            "(max rounds {} exceeded)",
            MAX_ROUNDS
        )));
        let _ = tx.send(AgentEvent::Error {
            message: format!("Max rounds ({MAX_ROUNDS}) exceeded"),
        });
        if let Some(ref hooks) = self.hook_registry {
            let ctx = HookContext::new().with_session(session_id.as_str());
            hooks.execute(HookPoint::SessionEnd, &ctx).await;
        }
        let _ = tx.send(AgentEvent::Done);
        Ok(())
    }
}

struct PendingToolUse {
    id: String,
    name: String,
    input_json: String,
}

/// Soft-trim tool result if it exceeds the limit (67% head + 27% tail).
fn maybe_trim_tool_result(result: &str) -> String {
    if result.len() <= TOOL_RESULT_SOFT_LIMIT {
        return result.to_string();
    }
    let head_end = result
        .char_indices()
        .nth(20_000)
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let char_count = result.chars().count();
    let tail_start = result
        .char_indices()
        .nth(char_count.saturating_sub(8_000))
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let omitted = result.len().saturating_sub(20_000 + 8_000);
    format!(
        "{}\n\n[... omitted {} chars ...]\n\n{}",
        &result[..head_end],
        omitted,
        &result[tail_start..]
    )
}
