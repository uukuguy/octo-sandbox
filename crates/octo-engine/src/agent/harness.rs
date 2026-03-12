//! Agent Harness — pure-function agent loop entry point.
//!
//! `run_agent_loop(config, messages)` replaces the monolithic `AgentLoop::run()`.
//! All dependencies are injected via `AgentLoopConfig`; the function returns
//! a `BoxStream<AgentEvent>` for fully decoupled consumption.

use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use std::sync::Arc;

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, StopReason, StreamEvent, ToolOutput,
};

use crate::security::SafetyDecision;
use crate::tools::approval::{ApprovalDecision, ApprovalGate, ApprovalManager};

use crate::context::{
    ContextPruner, DegradationLevel, MemoryFlusher, NewSystemPromptBuilder as SystemPromptBuilder,
};
use crate::hooks::{HookAction, HookContext, HookPoint};
use crate::providers::{LlmErrorKind, RetryPolicy};

use super::continuation::{ContinuationConfig, ContinuationTracker};
use super::deferred_action::DeferredActionDetector;
use super::events::{AgentEvent, AgentLoopResult, NormalizedStopReason};
use super::loop_config::AgentLoopConfig;
use super::loop_guard::LoopGuardVerdict;
use super::loop_steps;
use super::parallel::execute_parallel;
use super::CancellationToken;

const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

/// Maximum tool output size before truncation is applied.
const MAX_TOOL_OUTPUT_SIZE: usize = 100_000;

/// Default context window when no budget is configured (128K tokens).
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

/// Compute dynamic tool result budget based on context window size.
///
/// Returns `(soft_limit, hard_limit)` in characters:
/// - `soft_limit` = 15% of context_window, clamped to [8K, 50K]
/// - `hard_limit` = 30% of context_window, clamped to [30K, 200K]
///
/// Falls back to compile-time constants when context_window is 0.
fn tool_result_budget(context_window: usize) -> (usize, usize) {
    if context_window == 0 {
        return (TOOL_RESULT_SOFT_LIMIT, MAX_TOOL_OUTPUT_SIZE);
    }
    let soft = ((context_window as f64 * 0.15) as usize).clamp(8_000, 50_000);
    let hard = ((context_window as f64 * 0.30) as usize).clamp(30_000, 200_000);
    (soft, hard)
}

/// Extract context window from an `AgentLoopConfig`, falling back to default.
fn context_window_from_config(config: &AgentLoopConfig) -> usize {
    config
        .budget
        .as_ref()
        .map(|b| b.context_window() as usize)
        .filter(|&w| w > 0)
        .unwrap_or(DEFAULT_CONTEXT_WINDOW)
}

/// Pending tool call accumulated from stream events.
struct PendingToolUse {
    id: String,
    name: String,
    input_json: String,
}

/// Result of consuming the LLM stream for one iteration.
struct StreamResult {
    full_text: String,
    full_thinking: String,
    tool_uses: Vec<PendingToolUse>,
    stop_reason: StopReason,
    input_tokens: u32,
    #[allow(dead_code)]
    output_tokens: u32,
}

/// Pure-function agent loop entry point.
///
/// All dependencies injected via config; returns a stream of events.
/// The caller consumes the stream to observe agent progress.
pub fn run_agent_loop(
    config: AgentLoopConfig,
    messages: Vec<ChatMessage>,
) -> BoxStream<'static, AgentEvent> {
    let (tx, rx) = mpsc::channel(256);

    tokio::spawn(async move {
        run_agent_loop_inner(config, messages, tx).await;
    });

    tokio_stream::wrappers::ReceiverStream::new(rx).boxed()
}

async fn run_agent_loop_inner(
    config: AgentLoopConfig,
    mut messages: Vec<ChatMessage>,
    tx: mpsc::Sender<AgentEvent>,
) {
    info!(
        session = %config.session_id,
        "Harness: agent loop starting, {} messages in history",
        messages.len()
    );

    let provider = match config.provider {
        Some(ref p) => p.clone(),
        None => {
            let _ = tx
                .send(AgentEvent::Error {
                    message: "No provider configured".into(),
                })
                .await;
            let _ = tx.send(AgentEvent::Done).await;
            return;
        }
    };

    let tools = match config.tools {
        Some(ref t) => t.clone(),
        None => {
            let _ = tx
                .send(AgentEvent::Error {
                    message: "No tool registry configured".into(),
                })
                .await;
            let _ = tx.send(AgentEvent::Done).await;
            return;
        }
    };

    // --- Zone A: Build system prompt ---
    let system_prompt = {
        let mut builder = SystemPromptBuilder::new();
        if let Some(ref manifest) = config.manifest {
            builder = builder.with_manifest(manifest.clone());
        }
        if let Some(ref skills) = config.skills {
            builder = builder.with_skill_index(skills);
        }
        if let Some(ref active_skill) = config.active_skill {
            builder = builder.with_active_skill(active_skill);
        }
        let mut prompt = builder.build();
        // Canary injection: append token AFTER build to prevent override
        if let Some(ref canary) = config.canary_token {
            prompt.push_str("\n\n<!-- CANARY: ");
            prompt.push_str(canary);
            prompt.push_str(" -->");
        }
        prompt
    };
    debug!("System prompt length: {} chars", system_prompt.len());

    // --- Zone B: Inject working memory ---
    if let Some(ref memory) = config.memory {
        let memory_xml = memory
            .compile(&config.user_id, &config.sandbox_id)
            .await
            .unwrap_or_default();
        loop_steps::inject_zone_b(&mut messages, &memory_xml);
        debug!("Zone B injected: working memory {} chars", memory_xml.len());
    }

    // --- Compute tool specs ---
    let tool_specs = tools.specs();

    // --- Context management objects ---
    let mut budget = config.budget.clone().unwrap_or_default();
    let pruner = config.pruner.clone().unwrap_or_default();
    let mut loop_guard = config.loop_guard.clone().unwrap_or_default();

    // --- Dynamic tool result budget (W8-T8) ---
    let ctx_window = context_window_from_config(&config);
    let (tool_soft_limit, tool_hard_limit) = tool_result_budget(ctx_window);
    debug!(
        ctx_window,
        tool_soft_limit, tool_hard_limit, "Dynamic tool result budget computed"
    );

    // --- Compute max rounds ---
    let max_rounds = loop_steps::effective_max_rounds(config.max_iterations);

    // --- SessionStart hook ---
    if let Some(ref hooks) = config.hook_registry {
        let ctx = HookContext::new().with_session(config.session_id.as_str());
        hooks.execute(HookPoint::SessionStart, &ctx).await;
    }

    let mut total_tool_calls: u32 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    // P1-1: ContinuationTracker for max_tokens auto-continuation
    let mut continuation_tracker = ContinuationTracker::new(ContinuationConfig {
        max_continuations: config.max_tokens_continuation,
        ..Default::default()
    });

    // P1-4: DeferredActionDetector for detecting deferred actions in text
    let deferred_detector = DeferredActionDetector::new();
    let tool_ctx = match config.tool_ctx.clone() {
        Some(ctx) => ctx,
        None => {
            use std::path::PathBuf;
            octo_types::ToolContext {
                sandbox_id: config.sandbox_id.clone(),
                working_dir: PathBuf::from("."),
                path_validator: None,
            }
        }
    };

    // === Main loop ===
    for round in 0..max_rounds {
        debug!(round, "Harness: round starting");

        // --- Emit IterationStart ---
        let _ = tx.send(AgentEvent::IterationStart { round }).await;

        // --- PreTask hook (round 0 only) ---
        if round == 0 {
            if let Some(ref hooks) = config.hook_registry {
                let ctx = HookContext::new()
                    .with_session(config.session_id.as_str())
                    .with_turn(round);
                if let HookAction::Abort(reason) = hooks.execute(HookPoint::PreTask, &ctx).await {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: reason.clone(),
                        })
                        .await;
                    let _ = tx.send(AgentEvent::Done).await;
                    fire_session_end(&config, &tx).await;
                    return;
                }
            }
        }

        // --- LoopTurnStart hook ---
        if let Some(ref hooks) = config.hook_registry {
            let ctx = HookContext::new()
                .with_session(config.session_id.as_str())
                .with_turn(round);
            if let HookAction::Abort(reason) = hooks.execute(HookPoint::LoopTurnStart, &ctx).await {
                let _ = tx
                    .send(AgentEvent::Error {
                        message: reason.clone(),
                    })
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                fire_session_end(&config, &tx).await;
                return;
            }
        }

        // --- TelemetryBus: LoopTurnStarted ---
        if let Some(ref bus) = config.event_bus {
            bus.publish(crate::event::TelemetryEvent::LoopTurnStarted {
                session_id: config.session_id.as_str().to_string(),
                turn: round,
            })
            .await;
        }

        let turn_start = std::time::Instant::now();

        // --- Check cancellation ---
        if config.cancel_token.is_cancelled() {
            info!(session = %config.session_id, "Harness: cancelled");
            break;
        }

        // --- Context management (P0-7) ---
        let level = budget.compute_degradation_level(&system_prompt, &messages, &tool_specs);
        if level != DegradationLevel::None {
            debug!(?level, "Applying context degradation");

            if level >= DegradationLevel::OverflowCompaction
                && level != DegradationLevel::FinalError
            {
                let boundary = ContextPruner::find_compaction_boundary(&messages, 20_000);
                if boundary > 0 {
                    let _ = MemoryFlusher::flush(
                        &messages,
                        boundary,
                        &*provider,
                        config.memory.as_deref().unwrap_or_else(|| {
                            // This branch shouldn't be reached since memory flush requires memory
                            panic!("Memory required for flush")
                        }),
                        config.memory_store.as_deref(),
                        &config.model,
                        config.user_id.as_str(),
                    )
                    .await;

                    let _ = tx
                        .send(AgentEvent::MemoryFlushed {
                            facts_count: boundary,
                        })
                        .await;
                }
            }

            pruner.apply(&mut messages, level);

            let _ = tx
                .send(AgentEvent::ContextDegraded {
                    level: format!("{:?}", level),
                    usage_pct: (budget.usage_ratio(&system_prompt, &messages, &tool_specs) * 100.0)
                        as f32,
                })
                .await;

            // TelemetryBus: ContextDegraded
            if let Some(ref bus) = config.event_bus {
                bus.publish(crate::event::TelemetryEvent::ContextDegraded {
                    session_id: config.session_id.as_str().to_string(),
                    level: format!("{:?}", level),
                })
                .await;
            }

            if let Some(ref hooks) = config.hook_registry {
                let ctx = HookContext::new()
                    .with_session(config.session_id.as_str())
                    .with_turn(round)
                    .with_degradation(format!("{:?}", level));
                hooks.execute(HookPoint::ContextDegraded, &ctx).await;
            }
        }

        // TelemetryBus: TokenBudgetUpdated (every round)
        if let Some(ref bus) = config.event_bus {
            let ratio = budget.usage_ratio(&system_prompt, &messages, &tool_specs);
            let available = budget.available_space();
            let used = (ratio * available as f64) as u64;
            bus.publish(crate::event::TelemetryEvent::TokenBudgetUpdated {
                session_id: config.session_id.as_str().to_string(),
                used,
                total: available,
                ratio,
            })
            .await;
        }

        // --- AIDefence input check (round 0 only) ---
        if round == 0 {
            if let Some(ref defence) = config.defence {
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
                        warn!(violation = %violation, "AIDefence blocked input");
                        let _ = tx
                            .send(AgentEvent::SecurityBlocked {
                                reason: format!("Input security check failed: {violation}"),
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return;
                    }
                }
            }

            // --- SafetyPipeline input check (T3-8) ---
            if let Some(ref pipeline) = config.safety_pipeline {
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
                    match pipeline.check_input(text).await {
                        SafetyDecision::Block(reason) => {
                            warn!(reason = %reason, "SafetyPipeline blocked input");
                            let _ = tx
                                .send(AgentEvent::SecurityBlocked {
                                    reason: format!("Safety pipeline blocked input: {reason}"),
                                })
                                .await;
                            let _ = tx.send(AgentEvent::Done).await;
                            return;
                        }
                        SafetyDecision::Warn(msg) => {
                            warn!(msg = %msg, "SafetyPipeline input warning");
                        }
                        SafetyDecision::Sanitize(_) | SafetyDecision::Allow => {}
                    }
                }
            }
        }

        // --- Build CompletionRequest ---
        // P1-2: Apply observation masking to messages sent to LLM
        let masker = crate::context::ObservationMasker::with_defaults();
        let masked_messages = masker.mask(&messages);

        let force_text = loop_steps::should_force_text_only(
            round,
            config.max_iterations,
            config.force_text_at_last,
        );
        let request = CompletionRequest {
            model: config.model.clone(),
            system: Some(system_prompt.clone()),
            messages: masked_messages,
            max_tokens: config.max_tokens,
            temperature: None,
            tools: if force_text {
                vec![]
            } else {
                tool_specs.clone()
            },
            stream: true,
        };

        // --- Call provider with retry (P0-5) ---
        let retry_policy = RetryPolicy::default();
        let mut llm_stream = None;
        let mut last_err = None;
        let mut attempt = 0u32;

        loop {
            match provider.stream(request.clone()).await {
                Ok(s) => {
                    llm_stream = Some(s);
                    break;
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if retry_policy.should_retry_str(&err_str, attempt) {
                        let delay = retry_policy.delay_for(attempt);
                        let kind = LlmErrorKind::classify_from_str(&err_str);
                        warn!(
                            "LLM stream failed (attempt {}/{}, kind={:?}), retrying in {:?}: {}",
                            attempt + 1,
                            retry_policy.max_retries,
                            kind,
                            delay,
                            e
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

        let mut llm_stream = match llm_stream {
            Some(s) => s,
            None => {
                let e = last_err.unwrap_or_else(|| anyhow::anyhow!("stream failed"));
                let _ = tx
                    .send(AgentEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                fire_session_end(&config, &tx).await;
                return;
            }
        };

        // --- Consume stream (P0-5) ---
        let stream_result = consume_stream(&mut llm_stream, &tx, &config.agent_config).await;

        let stream_result = match stream_result {
            Ok(r) => r,
            Err(e) => {
                warn!("Stream error: {e}");
                let _ = tx
                    .send(AgentEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                fire_session_end(&config, &tx).await;
                return;
            }
        };

        // Update budget with actual usage
        budget.update_actual_usage(stream_result.input_tokens, messages.len());

        // Accumulate token usage across rounds
        total_input_tokens += stream_result.input_tokens as u64;
        total_output_tokens += stream_result.output_tokens as u64;

        // Emit budget snapshot
        let snapshot = budget.snapshot(&system_prompt, &messages, &tool_specs);
        let _ = tx
            .send(AgentEvent::TokenBudgetUpdate { budget: snapshot })
            .await;

        let StreamResult {
            mut full_text,
            mut full_thinking,
            tool_uses,
            stop_reason,
            ..
        } = stream_result;

        // Thinking fallback: if no text but has thinking, use thinking as text
        if full_text.is_empty() && !full_thinking.is_empty() {
            debug!("No text content, falling back thinking to text");
            full_text = full_thinking.clone();
            full_thinking.clear();
        }

        // Emit ThinkingComplete
        if !full_thinking.is_empty() {
            let _ = tx
                .send(AgentEvent::ThinkingComplete {
                    text: full_thinking,
                })
                .await;
        }

        // --- If no tool uses: check for continuation or finalize ---
        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
            // P1-1: Auto-continuation on max_tokens
            if stop_reason == StopReason::MaxTokens
                && continuation_tracker.should_continue("max_tokens")
            {
                let prompt = continuation_tracker.record_continuation(full_text.len());
                debug!(
                    count = continuation_tracker.continuation_count(),
                    "Continuation: injecting prompt"
                );
                // Append assistant text so far + continuation prompt
                messages.push(ChatMessage::assistant(&full_text));
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: prompt }],
                });
                let _ = tx.send(AgentEvent::IterationEnd { round }).await;
                continue; // Re-enter loop for next LLM call
            }

            // AIDefence output validation
            if !full_text.is_empty() {
                if let Some(ref defence) = config.defence {
                    if let Err(violation) = defence.check_output(&full_text) {
                        warn!(violation = %violation, "AIDefence blocked output");
                        let _ = tx
                            .send(AgentEvent::SecurityBlocked {
                                reason: format!("Output security check failed: {violation}"),
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return;
                    }
                }
            }

            // --- SafetyPipeline output check (T3-8) ---
            if !full_text.is_empty() {
                if let Some(ref pipeline) = config.safety_pipeline {
                    match pipeline.check_output(&full_text).await {
                        SafetyDecision::Block(reason) => {
                            warn!(reason = %reason, "SafetyPipeline blocked output");
                            let _ = tx
                                .send(AgentEvent::SecurityBlocked {
                                    reason: format!("Safety pipeline blocked output: {reason}"),
                                })
                                .await;
                            let _ = tx.send(AgentEvent::Done).await;
                            return;
                        }
                        SafetyDecision::Sanitize(cleaned) => {
                            debug!("SafetyPipeline sanitized output");
                            full_text = cleaned;
                        }
                        SafetyDecision::Warn(msg) => {
                            warn!(msg = %msg, "SafetyPipeline output warning");
                        }
                        SafetyDecision::Allow => {}
                    }
                }
            }

            // P1-4: Detect deferred actions in final text
            if !full_text.is_empty() {
                let deferred_matches = deferred_detector.detect(&full_text);
                for m in &deferred_matches {
                    debug!(
                        category = ?m.category,
                        "Deferred action detected: {}",
                        m.text
                    );
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: format!(
                                "Deferred action detected ({:?}): {}",
                                m.category, m.text
                            ),
                        })
                        .await;
                }
            }

            messages.push(ChatMessage::assistant(if full_text.is_empty() {
                "(no response)"
            } else {
                &full_text
            }));

            if !full_text.is_empty() {
                let _ = tx
                    .send(AgentEvent::TextComplete {
                        text: full_text.clone(),
                    })
                    .await;
            }

            // Typing stop
            if config.agent_config.enable_typing_signal {
                let _ = tx.send(AgentEvent::Typing { state: false }).await;
            }

            let _ = tx.send(AgentEvent::IterationEnd { round }).await;

            fire_post_task_hooks(&config, &tx, round, turn_start.elapsed().as_millis() as u64)
                .await;

            let _ = tx
                .send(AgentEvent::Completed(AgentLoopResult {
                    rounds: round + 1,
                    tool_calls: total_tool_calls,
                    stop_reason: NormalizedStopReason::from(stop_reason),
                    input_tokens: total_input_tokens,
                    output_tokens: total_output_tokens,
                    final_messages: messages.clone(),
                }))
                .await;
            let _ = tx.send(AgentEvent::Done).await;
            return;
        }

        // --- Build assistant message with text + tool_use blocks ---
        let mut assistant_content: Vec<ContentBlock> = Vec::new();
        if !full_text.is_empty() {
            assistant_content.push(ContentBlock::Text {
                text: full_text.clone(),
            });
        }
        let parsed_tools: Vec<_> = tool_uses
            .iter()
            .map(|tu| {
                let input: serde_json::Value =
                    serde_json::from_str(&tu.input_json).unwrap_or_default();
                (tu, input)
            })
            .collect();
        for (tu, input) in &parsed_tools {
            assistant_content.push(ContentBlock::ToolUse {
                id: tu.id.clone(),
                name: tu.name.clone(),
                input: input.clone(),
            });
        }
        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: assistant_content,
        });

        // --- P1-3: ToolCallInterceptor check ---
        if let Some(ref interceptor) = config.interceptor {
            for (tu, _input) in &parsed_tools {
                if let Err(trust_err) = interceptor.check_permission(&tu.name) {
                    warn!(tool = %tu.name, "Tool blocked by skill constraint: {trust_err}");
                    // Replace with error result instead of blocking entirely
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: format!(
                                "Tool '{}' blocked by skill constraint: {trust_err}",
                                tu.name
                            ),
                        })
                        .await;
                }
            }
        }

        // --- Loop Guard check (P0-6) ---
        let mut guard_blocked = false;
        for (tu, input) in &parsed_tools {
            let verdict = loop_guard.check(&tu.name, input);
            match &verdict {
                LoopGuardVerdict::Block(msg) | LoopGuardVerdict::CircuitBreak(msg) => {
                    warn!("Loop Guard blocked: {}", msg);
                    // TelemetryBus: LoopGuardTriggered
                    if let Some(ref bus) = config.event_bus {
                        bus.publish(crate::event::TelemetryEvent::LoopGuardTriggered {
                            session_id: config.session_id.as_str().to_string(),
                            reason: msg.clone(),
                        })
                        .await;
                    }
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: format!("Loop Guard: {}", msg),
                        })
                        .await;
                    let _ = tx.send(AgentEvent::Done).await;
                    fire_session_end(&config, &tx).await;
                    guard_blocked = true;
                    break;
                }
                LoopGuardVerdict::Warn(msg) => {
                    warn!("Loop Guard warning: {}", msg);
                }
                LoopGuardVerdict::Allow => {}
            }
        }
        if guard_blocked {
            return;
        }

        // --- Send ToolStart events ---
        for (tu, input) in &parsed_tools {
            let _ = tx
                .send(AgentEvent::ToolStart {
                    tool_id: tu.id.clone(),
                    tool_name: tu.name.clone(),
                    input: input.clone(),
                })
                .await;
            if let Some(ref bus) = config.event_bus {
                bus.publish(crate::event::TelemetryEvent::ToolCallStarted {
                    session_id: config.session_id.as_str().to_string(),
                    tool_name: tu.name.clone(),
                })
                .await;
            }
        }

        // --- Execute tools (P0-6) ---
        let cancellation_token = CancellationToken::new();

        // Build a default ApprovalManager if none was injected (dev mode = auto-approve).
        let approval_mgr = config
            .approval_manager
            .as_ref()
            .map(|m| m.clone())
            .unwrap_or_else(|| Arc::new(ApprovalManager::dev_mode()));

        let tool_outputs: Vec<_> = if config.agent_config.enable_parallel {
            let tools_to_run: Vec<_> = parsed_tools
                .iter()
                .map(|(tu, input)| (tu.name.clone(), input.clone()))
                .collect();

            // --- T3-4: Approval check for parallel tools (before batch execution) ---
            let mut approval_blocked = false;
            for (tu, _input) in &parsed_tools {
                if let Some(tool) = tools.get(&tu.name) {
                    let requirement = tool.approval();
                    let risk = tool.risk_level();
                    let decision = approval_mgr.check_requirement(&tu.name, requirement, risk);
                    match decision {
                        ApprovalDecision::NeedsApproval { reason, .. } => {
                            info!(tool = %tu.name, %reason, "Tool requires approval (parallel)");
                            let approved = request_approval(
                                &tu.name,
                                &tu.id,
                                risk,
                                &tx,
                                &config.approval_gate,
                            )
                            .await;
                            if !approved {
                                warn!(tool = %tu.name, "Tool approval denied or timed out");
                                let _ = tx
                                    .send(AgentEvent::Error {
                                        message: format!(
                                            "Tool '{}' execution denied: approval rejected/timed out",
                                            tu.name
                                        ),
                                    })
                                    .await;
                                let _ = tx.send(AgentEvent::Done).await;
                                fire_session_end(&config, &tx).await;
                                approval_blocked = true;
                                break;
                            }
                        }
                        ApprovalDecision::Denied { reason } => {
                            warn!(tool = %tu.name, %reason, "Tool denied by approval policy");
                            let _ = tx
                                .send(AgentEvent::Error {
                                    message: format!(
                                        "Tool '{}' denied: {reason}",
                                        tu.name
                                    ),
                                })
                                .await;
                            let _ = tx.send(AgentEvent::Done).await;
                            fire_session_end(&config, &tx).await;
                            approval_blocked = true;
                            break;
                        }
                        ApprovalDecision::Approved => {
                            debug!(tool = %tu.name, "Tool auto-approved");
                        }
                    }
                }
            }
            if approval_blocked {
                return;
            }

            let config_timeout = if config.tool_timeout_secs > 0 {
                Some(config.tool_timeout_secs)
            } else {
                None
            };
            let results = execute_parallel(
                tools_to_run,
                &tools,
                config.agent_config.max_parallel_tools,
                &cancellation_token,
                &tool_ctx,
                config_timeout,
            )
            .await;

            parsed_tools
                .iter()
                .zip(results)
                .map(|((tu, input), (_, result))| {
                    // Post-process parallel results: duration is already recorded
                    // by the parallel executor, but we add truncation + metadata
                    let duration_ms = result.duration_ms;
                    let result = postprocess_tool_output(
                        result,
                        &tu.name,
                        config.session_id.as_str(),
                        duration_ms, // preserve duration if already set
                        tool_hard_limit,
                    );
                    (tu, input.clone(), result)
                })
                .collect()
        } else {
            let mut outputs = Vec::new();
            for (tu, input) in &parsed_tools {
                // --- T3-4: Approval check for sequential tools ---
                if let Some(tool) = tools.get(&tu.name) {
                    let requirement = tool.approval();
                    let risk = tool.risk_level();
                    let decision = approval_mgr.check_requirement(&tu.name, requirement, risk);
                    match decision {
                        ApprovalDecision::NeedsApproval { reason, .. } => {
                            info!(tool = %tu.name, %reason, "Tool requires approval");
                            let approved = request_approval(
                                &tu.name,
                                &tu.id,
                                risk,
                                &tx,
                                &config.approval_gate,
                            )
                            .await;
                            if !approved {
                                warn!(tool = %tu.name, "Tool approval denied or timed out");
                                let _ = tx
                                    .send(AgentEvent::Error {
                                        message: format!(
                                            "Tool '{}' execution denied: approval rejected/timed out",
                                            tu.name
                                        ),
                                    })
                                    .await;
                                let _ = tx.send(AgentEvent::Done).await;
                                fire_session_end(&config, &tx).await;
                                return;
                            }
                        }
                        ApprovalDecision::Denied { reason } => {
                            warn!(tool = %tu.name, %reason, "Tool denied by approval policy");
                            let _ = tx
                                .send(AgentEvent::Error {
                                    message: format!("Tool '{}' denied: {reason}", tu.name),
                                })
                                .await;
                            let _ = tx.send(AgentEvent::Done).await;
                            fire_session_end(&config, &tx).await;
                            return;
                        }
                        ApprovalDecision::Approved => {
                            debug!(tool = %tu.name, "Tool auto-approved");
                        }
                    }
                }

                // PreToolUse hook
                if let Some(ref hooks) = config.hook_registry {
                    let ctx = HookContext::new()
                        .with_session(config.session_id.as_str())
                        .with_tool(&tu.name, input.clone());
                    if let HookAction::Block(reason) =
                        hooks.execute(HookPoint::PreToolUse, &ctx).await
                    {
                        warn!(tool = %tu.name, reason = %reason, "PreToolUse hook blocked tool");
                        let _ = tx
                            .send(AgentEvent::Error {
                                message: format!(
                                    "Tool '{}' blocked by security policy: {reason}",
                                    tu.name
                                ),
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return;
                    }
                }

                let exec_start = std::time::Instant::now();

                let result = if let Some(tool) = tools.get(&tu.name) {
                    match tool.execute(input.clone(), &tool_ctx).await {
                        Ok(r) => r,
                        Err(e) => ToolOutput::error(format!("Tool error: {e}")),
                    }
                } else {
                    ToolOutput::error(format!("Unknown tool: {}", tu.name))
                };

                let exec_duration = exec_start.elapsed().as_millis() as u64;

                // Post-process: duration, truncation, metadata
                let result = postprocess_tool_output(
                    result,
                    &tu.name,
                    config.session_id.as_str(),
                    exec_duration,
                    tool_hard_limit,
                );

                // PostToolUse hook
                if let Some(ref hooks) = config.hook_registry {
                    let mut ctx = HookContext::new()
                        .with_session(config.session_id.as_str())
                        .with_tool(&tu.name, input.clone())
                        .with_result(!result.is_error, exec_duration);
                    ctx.tool_result = Some(serde_json::Value::String(result.content.clone()));
                    hooks.execute(HookPoint::PostToolUse, &ctx).await;
                }

                if let Some(ref bus) = config.event_bus {
                    bus.publish(crate::event::TelemetryEvent::ToolCallCompleted {
                        session_id: config.session_id.as_str().to_string(),
                        tool_name: tu.name.clone(),
                        duration_ms: exec_duration,
                    })
                    .await;
                }

                outputs.push((tu, input.clone(), result));
            }
            outputs
        };

        // --- Process tool results ---
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        for (tu, input, result) in tool_outputs {
            total_tool_calls += 1;

            let _ = tx
                .send(AgentEvent::ToolResult {
                    tool_id: tu.id.clone(),
                    output: result.content.clone(),
                    success: !result.is_error,
                })
                .await;

            let trimmed_output = soft_trim_tool_result(&result.content, tool_soft_limit);

            // AIDefence injection check on tool results
            if let Some(ref defence) = config.defence {
                if let Err(violation) = defence.check_injection(&trimmed_output) {
                    warn!(
                        tool = %tu.name,
                        violation = %violation,
                        "AIDefence detected injection in tool result"
                    );
                    let _ = tx
                        .send(AgentEvent::SecurityBlocked {
                            reason: format!(
                                "Tool '{}' result contains injection attempt: {violation}",
                                tu.name
                            ),
                        })
                        .await;
                    let _ = tx.send(AgentEvent::Done).await;
                    return;
                }
            }

            // --- SafetyPipeline tool result check (T3-8) ---
            let trimmed_output = if let Some(ref pipeline) = config.safety_pipeline {
                match pipeline.check_tool_result(&tu.name, &trimmed_output).await {
                    SafetyDecision::Block(reason) => {
                        warn!(tool = %tu.name, reason = %reason, "SafetyPipeline blocked tool result");
                        let _ = tx
                            .send(AgentEvent::SecurityBlocked {
                                reason: format!(
                                    "Safety pipeline blocked tool '{}' result: {reason}",
                                    tu.name
                                ),
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        return;
                    }
                    SafetyDecision::Sanitize(cleaned) => {
                        debug!(tool = %tu.name, "SafetyPipeline sanitized tool result");
                        cleaned
                    }
                    SafetyDecision::Warn(msg) => {
                        warn!(tool = %tu.name, msg = %msg, "SafetyPipeline tool result warning");
                        trimmed_output
                    }
                    SafetyDecision::Allow => trimmed_output,
                }
            } else {
                trimmed_output
            };

            // Record outcome for loop detection
            if let Some(outcome_warning) =
                loop_guard.record_outcome(&tu.name, &input, &result.content)
            {
                warn!("Loop Guard outcome: {}", outcome_warning);
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

        // --- IterationEnd + LoopTurnEnd hook ---
        let _ = tx.send(AgentEvent::IterationEnd { round }).await;

        if let Some(ref hooks) = config.hook_registry {
            let elapsed = turn_start.elapsed().as_millis() as u64;
            let ctx = HookContext::new()
                .with_session(config.session_id.as_str())
                .with_turn(round)
                .with_result(true, elapsed);
            hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
        }

        // Typing stop
        if config.agent_config.enable_typing_signal {
            let _ = tx.send(AgentEvent::Typing { state: false }).await;
        }
    }

    // --- Max rounds exceeded ---
    warn!("Max rounds ({max_rounds}) exceeded");
    messages.push(ChatMessage::assistant(format!(
        "(max rounds {max_rounds} exceeded)"
    )));
    let _ = tx
        .send(AgentEvent::Error {
            message: format!("Max rounds ({max_rounds}) exceeded"),
        })
        .await;
    let _ = tx
        .send(AgentEvent::Completed(AgentLoopResult {
            rounds: max_rounds,
            tool_calls: total_tool_calls,
            stop_reason: NormalizedStopReason::MaxIterations,
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            final_messages: messages.clone(),
        }))
        .await;
    fire_session_end(&config, &tx).await;
    let _ = tx.send(AgentEvent::Done).await;
}

/// Consume the LLM stream, accumulating text/thinking/tool_uses.
async fn consume_stream(
    stream: &mut BoxStream<'_, anyhow::Result<StreamEvent>>,
    tx: &mpsc::Sender<AgentEvent>,
    agent_config: &super::config::AgentConfig,
) -> anyhow::Result<StreamResult> {
    let mut full_text = String::new();
    let mut full_thinking = String::new();
    let mut tool_uses: Vec<PendingToolUse> = Vec::new();
    let mut current_tool: Option<PendingToolUse> = None;
    let mut sent_typing = false;
    let mut stop_reason = StopReason::EndTurn;
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::MessageStart { .. }) => {}
            Ok(StreamEvent::TextDelta { text }) => {
                if !sent_typing && agent_config.enable_typing_signal {
                    let _ = tx.send(AgentEvent::Typing { state: true }).await;
                    sent_typing = true;
                }
                full_text.push_str(&text);
                let _ = tx.send(AgentEvent::TextDelta { text }).await;
            }
            Ok(StreamEvent::ThinkingDelta { text }) => {
                if !sent_typing && agent_config.enable_typing_signal {
                    let _ = tx.send(AgentEvent::Typing { state: true }).await;
                    sent_typing = true;
                }
                full_thinking.push_str(&text);
                let _ = tx.send(AgentEvent::ThinkingDelta { text }).await;
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
            Ok(StreamEvent::MessageStop {
                stop_reason: sr,
                usage,
            }) => {
                debug!(
                    ?sr,
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    "Message complete"
                );
                stop_reason = sr;
                input_tokens = usage.input_tokens;
                output_tokens = usage.output_tokens;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // Finalize any in-progress tool from InputDelta (shouldn't happen normally)
    if let Some(tool) = current_tool {
        tool_uses.push(tool);
    }

    Ok(StreamResult {
        full_text,
        full_thinking,
        tool_uses,
        stop_reason,
        input_tokens,
        output_tokens,
    })
}

/// Fire SessionEnd hook.
async fn fire_session_end(config: &AgentLoopConfig, _tx: &mpsc::Sender<AgentEvent>) {
    if let Some(ref hooks) = config.hook_registry {
        let ctx = HookContext::new().with_session(config.session_id.as_str());
        hooks.execute(HookPoint::SessionEnd, &ctx).await;
    }
}

/// Fire PostTask + LoopTurnEnd + SessionEnd hooks at end of successful turn.
async fn fire_post_task_hooks(
    config: &AgentLoopConfig,
    tx: &mpsc::Sender<AgentEvent>,
    round: u32,
    elapsed_ms: u64,
) {
    if let Some(ref hooks) = config.hook_registry {
        let ctx = HookContext::new()
            .with_session(config.session_id.as_str())
            .with_turn(round)
            .with_result(true, elapsed_ms);
        hooks.execute(HookPoint::PostTask, &ctx).await;
        hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
        hooks.execute(HookPoint::SessionEnd, &ctx).await;
    }
    let _ = tx; // used for potential future events
}

/// Post-process a tool output: record duration, handle truncation, inject metadata.
fn postprocess_tool_output(
    mut output: ToolOutput,
    tool_name: &str,
    session_id: &str,
    elapsed_ms: u64,
    hard_limit: usize,
) -> ToolOutput {
    // 1. Record execution duration
    output.duration_ms = elapsed_ms;

    // 2. Truncation handling: if content exceeds hard_limit, truncate
    if output.content.len() > hard_limit {
        let original_size = output.content.len();
        output.content.truncate(hard_limit);
        output = output.mark_truncated(original_size);
        debug!(
            tool = tool_name,
            original_size,
            "Tool output truncated to {} bytes",
            hard_limit
        );
    }

    // 3. Metadata injection: add tool_name and session_id if not already set
    if output.metadata.is_none() {
        output = output.with_metadata(serde_json::json!({
            "tool_name": tool_name,
            "session_id": session_id,
        }));
    }

    output
}

/// Request human approval for a tool invocation.
///
/// Emits an `AgentEvent::ApprovalRequired` and waits for a response via the
/// `ApprovalGate`. If no gate is configured, auto-rejects (safe default).
async fn request_approval(
    tool_name: &str,
    tool_id: &str,
    risk_level: octo_types::RiskLevel,
    tx: &mpsc::Sender<AgentEvent>,
    gate: &Option<ApprovalGate>,
) -> bool {
    // Emit the approval request event to consumers
    let _ = tx
        .send(AgentEvent::ApprovalRequired {
            tool_name: tool_name.to_string(),
            tool_id: tool_id.to_string(),
            risk_level,
        })
        .await;

    // If there's an approval gate, register and wait for the response
    if let Some(gate) = gate {
        let rx = gate.register(tool_id).await;
        info!(
            tool_name,
            tool_id, "Waiting for human approval via ApprovalGate"
        );
        ApprovalGate::wait_for_approval(rx).await
    } else {
        // No gate configured — auto-reject for safety
        warn!(
            tool_name,
            tool_id, "No ApprovalGate configured, auto-rejecting"
        );
        false
    }
}

/// Soft-trim tool result if it exceeds the limit (67% head + 27% tail).
fn soft_trim_tool_result(result: &str, soft_limit: usize) -> String {
    if result.len() <= soft_limit {
        return result.to_string();
    }
    // Distribute: 67% head, 27% tail (remaining 6% for omission marker)
    let head_chars = (soft_limit as f64 * 0.67) as usize;
    let tail_chars = (soft_limit as f64 * 0.27) as usize;
    let head_end = result
        .char_indices()
        .nth(head_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let char_count = result.chars().count();
    let tail_start = result
        .char_indices()
        .nth(char_count.saturating_sub(tail_chars))
        .map(|(idx, _)| idx)
        .unwrap_or(result.len());
    let omitted = result.len().saturating_sub(head_chars + tail_chars);
    format!(
        "{}\n\n[... omitted {} chars ...]\n\n{}",
        &result[..head_end],
        omitted,
        &result[tail_start..]
    )
}

// ---------------------------------------------------------------------------
// Text-based tool call recovery (W7-T3)
// ---------------------------------------------------------------------------
// Some LLMs (open-source models via OpenAI-compatible API) emit tool calls as
// plain text instead of structured `tool_use` blocks.  The functions below
// attempt to parse those text-based invocations so the agent loop can still
// execute the requested tools.
//
// Integration point: after `consume_stream` returns, if `tool_uses` is empty
// but `full_text` is non-empty, call `parse_tool_calls_from_text(&full_text)`
// and merge the results into the tool_uses vector.
// ---------------------------------------------------------------------------

/// Deserialization helper for JSON-based text tool calls.
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TextToolCall {
    name: String,
    arguments: serde_json::Value,
}

/// Attempt to recover tool calls embedded in plain-text LLM output.
///
/// Two strategies are tried in order:
/// 1. **JSON block parsing** -- matches fenced ```json ... ``` blocks or bare
///    JSON objects containing `{"name": "...", "arguments": {...}}`.
/// 2. **XML format** -- matches `<tool_name>...</tool_name>` where the inner
///    content is valid JSON arguments.
///
/// Returns an empty `Vec` when no tool calls can be recovered.
#[allow(dead_code)]
fn parse_tool_calls_from_text(text: &str) -> Vec<PendingToolUse> {
    use std::sync::OnceLock;
    use regex::Regex;

    // --- Strategy 1: JSON blocks ---
    static RE_JSON_FENCED: OnceLock<Regex> = OnceLock::new();
    static RE_JSON_BARE: OnceLock<Regex> = OnceLock::new();

    let re_fenced = RE_JSON_FENCED.get_or_init(|| {
        Regex::new(r"(?s)```(?:json)?\s*(\{.*?\})\s*```").expect("valid regex")
    });
    let re_bare = RE_JSON_BARE.get_or_init(|| {
        Regex::new(r#"(?s)\{\s*"name"\s*:\s*"[^"]+"\s*,\s*"arguments"\s*:\s*\{[^}]*\}\s*\}"#)
            .expect("valid regex")
    });

    let mut results = Vec::new();

    // Try fenced JSON blocks first
    for cap in re_fenced.captures_iter(text) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(tc) = serde_json::from_str::<TextToolCall>(json_str.as_str()) {
                results.push(PendingToolUse {
                    id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                    name: tc.name,
                    input_json: tc.arguments.to_string(),
                });
            }
        }
    }

    // If fenced blocks yielded nothing, try bare JSON objects
    if results.is_empty() {
        for m in re_bare.find_iter(text) {
            if let Ok(tc) = serde_json::from_str::<TextToolCall>(m.as_str()) {
                results.push(PendingToolUse {
                    id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                    name: tc.name,
                    input_json: tc.arguments.to_string(),
                });
            }
        }
    }

    // --- Strategy 2: XML format ---
    if results.is_empty() {
        static RE_XML: OnceLock<Regex> = OnceLock::new();
        let re_xml = RE_XML.get_or_init(|| {
            Regex::new(r"(?s)<([a-zA-Z_][a-zA-Z0-9_-]*)>([^<]*)</([a-zA-Z_][a-zA-Z0-9_-]*)>").expect("valid regex")
        });

        for cap in re_xml.captures_iter(text) {
            let open_tag = cap.get(1).map(|m| m.as_str());
            let close_tag = cap.get(3).map(|m| m.as_str());
            let inner = cap.get(2).map(|m| m.as_str().trim());
            // Ensure opening and closing tags match
            if open_tag != close_tag { continue; }
            let tool_name = open_tag.map(|s| s.to_string());
            if let (Some(name), Some(body)) = (tool_name, inner) {
                if let Ok(args) = serde_json::from_str::<serde_json::Value>(body) {
                    if args.is_object() {
                        results.push(PendingToolUse {
                            id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                            name,
                            input_json: args.to_string(),
                        });
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_calls_json_fenced_block() {
        let text = "Sure, let me run that.\n```json\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls -la\"}}\n```\n";
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        assert!(calls[0].id.starts_with("text-recovery-"));
        let args: serde_json::Value = serde_json::from_str(&calls[0].input_json).unwrap();
        assert_eq!(args["command"], "ls -la");
    }

    #[test]
    fn test_parse_tool_calls_bare_json() {
        let text = r#"I'll use: {"name": "file_read", "arguments": {"path": "/tmp/x"}} to read."#;
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "file_read");
    }

    #[test]
    fn test_parse_tool_calls_xml_format() {
        let text = "Execute:\n<bash>{\"command\": \"echo hello\"}</bash>\nDone.";
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        let args: serde_json::Value = serde_json::from_str(&calls[0].input_json).unwrap();
        assert_eq!(args["command"], "echo hello");
    }

    #[test]
    fn test_parse_tool_calls_plain_text_returns_empty() {
        let text = "Just a normal response with no tool calls at all.";
        let calls = parse_tool_calls_from_text(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_invalid_json_returns_empty() {
        let text = "```json\n{\"name\":\"bash\",\"arguments\":{\"command\": broken\n```";
        let calls = parse_tool_calls_from_text(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_multiple_fenced_blocks() {
        let text = "```json\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}\n```\nAlso:\n```json\n{\"name\":\"file_read\",\"arguments\":{\"path\":\"/tmp/x\"}}\n```\n";
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "bash");
        assert_eq!(calls[1].name, "file_read");
    }

    // --- W8-T8: Dynamic tool result budget tests ---

    #[test]
    fn test_tool_result_budget_small_context() {
        let (soft, hard) = tool_result_budget(32_000);
        assert_eq!(soft, 8_000); // 32K * 0.15 = 4.8K, clamped to 8K min
        assert_eq!(hard, 30_000); // 32K * 0.30 = 9.6K, clamped to 30K min
    }

    #[test]
    fn test_tool_result_budget_large_context() {
        let (soft, hard) = tool_result_budget(200_000);
        assert_eq!(soft, 30_000); // 200K * 0.15 = 30K
        assert_eq!(hard, 60_000); // 200K * 0.30 = 60K
    }

    #[test]
    fn test_tool_result_budget_very_large() {
        let (soft, hard) = tool_result_budget(1_000_000);
        assert_eq!(soft, 50_000); // clamped to 50K max
        assert_eq!(hard, 200_000); // clamped to 200K max
    }

    #[test]
    fn test_tool_result_budget_zero_fallback() {
        let (soft, hard) = tool_result_budget(0);
        assert_eq!(soft, TOOL_RESULT_SOFT_LIMIT);
        assert_eq!(hard, MAX_TOOL_OUTPUT_SIZE);
    }

    #[test]
    fn test_tool_result_budget_default_128k() {
        let (soft, hard) = tool_result_budget(DEFAULT_CONTEXT_WINDOW);
        // 128K * 0.15 = 19.2K (within [8K, 50K])
        assert_eq!(soft, 19_200);
        // 128K * 0.30 = 38.4K (within [30K, 200K])
        assert_eq!(hard, 38_400);
    }

    #[test]
    fn test_soft_trim_within_limit() {
        let input = "hello world";
        let result = soft_trim_tool_result(input, 100);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_soft_trim_exceeds_limit() {
        let input = "a".repeat(10_000);
        let result = soft_trim_tool_result(&input, 1_000);
        assert!(result.contains("[... omitted"));
        assert!(result.len() < input.len());
    }
}
