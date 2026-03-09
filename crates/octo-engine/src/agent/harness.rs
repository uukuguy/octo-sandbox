//! Agent Harness — pure-function agent loop entry point.
//!
//! `run_agent_loop(config, messages)` replaces the monolithic `AgentLoop::run()`.
//! All dependencies are injected via `AgentLoopConfig`; the function returns
//! a `BoxStream<AgentEvent>` for fully decoupled consumption.

use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, StopReason, StreamEvent, ToolResult,
};

use crate::context::{
    ContextPruner, DegradationLevel, MemoryFlusher,
    NewSystemPromptBuilder as SystemPromptBuilder,
};
use crate::hooks::{HookAction, HookContext, HookPoint};
use crate::providers::{LlmErrorKind, RetryPolicy};

use super::events::{AgentEvent, AgentLoopResult, NormalizedStopReason};
use super::loop_config::AgentLoopConfig;
use super::loop_guard::LoopGuardVerdict;
use super::loop_steps;
use super::parallel::execute_parallel;
use super::CancellationToken;

const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

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
    let system_prompt = if let Some(ref manifest) = config.manifest {
        SystemPromptBuilder::new()
            .with_manifest(manifest.clone())
            .build()
    } else {
        SystemPromptBuilder::new().build()
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
    let mut budget = config
        .budget
        .clone()
        .unwrap_or_default();
    let pruner = config
        .pruner
        .clone()
        .unwrap_or_else(|| ContextPruner::new());
    let mut loop_guard = config
        .loop_guard
        .clone()
        .unwrap_or_else(|| super::loop_guard::LoopGuard::new());

    // --- Compute max rounds ---
    let max_rounds = loop_steps::effective_max_rounds(config.max_iterations);

    // --- SessionStart hook ---
    if let Some(ref hooks) = config.hook_registry {
        let ctx = HookContext::new().with_session(config.session_id.as_str());
        hooks.execute(HookPoint::SessionStart, &ctx).await;
    }

    let mut total_tool_calls: u32 = 0;
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
            if let HookAction::Abort(reason) =
                hooks.execute(HookPoint::LoopTurnStart, &ctx).await
            {
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

        // --- EventBus: LoopTurnStarted ---
        if let Some(ref bus) = config.event_bus {
            bus.publish(crate::event::OctoEvent::LoopTurnStarted {
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
                        &mut messages,
                        boundary,
                        &*provider,
                        config.memory.as_ref().map(|m| &**m).unwrap_or_else(|| {
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
                    usage_pct: (budget.usage_ratio(&system_prompt, &messages, &tool_specs) * 100.0) as f32,
                })
                .await;

            if let Some(ref hooks) = config.hook_registry {
                let ctx = HookContext::new()
                    .with_session(config.session_id.as_str())
                    .with_turn(round)
                    .with_degradation(format!("{:?}", level));
                hooks.execute(HookPoint::ContextDegraded, &ctx).await;
            }
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
        }

        // --- Build CompletionRequest ---
        let force_text = loop_steps::should_force_text_only(
            round,
            config.max_iterations,
            config.force_text_at_last,
        );
        let request = CompletionRequest {
            model: config.model.clone(),
            system: Some(system_prompt.clone()),
            messages: messages.clone(),
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
        let stream_result =
            consume_stream(&mut llm_stream, &tx, &config.agent_config).await;

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

        // --- If no tool uses: final response ---
        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
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

            let _ = tx
                .send(AgentEvent::IterationEnd { round })
                .await;

            fire_post_task_hooks(&config, &tx, round, turn_start.elapsed().as_millis() as u64)
                .await;

            let _ = tx
                .send(AgentEvent::Completed(AgentLoopResult {
                    rounds: round + 1,
                    tool_calls: total_tool_calls,
                    stop_reason: NormalizedStopReason::from(stop_reason),
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

        // --- Loop Guard check (P0-6) ---
        let mut guard_blocked = false;
        for (tu, input) in &parsed_tools {
            let verdict = loop_guard.check(&tu.name, input);
            match &verdict {
                LoopGuardVerdict::Block(msg) | LoopGuardVerdict::CircuitBreak(msg) => {
                    warn!("Loop Guard blocked: {}", msg);
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
                bus.publish(crate::event::OctoEvent::ToolCallStarted {
                    session_id: config.session_id.as_str().to_string(),
                    tool_name: tu.name.clone(),
                })
                .await;
            }
        }

        // --- Execute tools (P0-6) ---
        let cancellation_token = CancellationToken::new();

        let tool_outputs: Vec<_> = if config.agent_config.enable_parallel {
            let tools_to_run: Vec<_> = parsed_tools
                .iter()
                .map(|(tu, input)| (tu.name.clone(), input.clone()))
                .collect();

            let results = execute_parallel(
                tools_to_run,
                &tools,
                config.agent_config.max_parallel_tools,
                &cancellation_token,
                &tool_ctx,
            )
            .await;

            parsed_tools
                .iter()
                .zip(results)
                .map(|((tu, input), (_, result))| (tu, input.clone(), result))
                .collect()
        } else {
            let mut outputs = Vec::new();
            for (tu, input) in &parsed_tools {
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
                        Err(e) => ToolResult::error(format!("Tool error: {e}")),
                    }
                } else {
                    ToolResult::error(format!("Unknown tool: {}", tu.name))
                };

                let exec_duration = exec_start.elapsed().as_millis() as u64;

                // PostToolUse hook
                if let Some(ref hooks) = config.hook_registry {
                    let mut ctx = HookContext::new()
                        .with_session(config.session_id.as_str())
                        .with_tool(&tu.name, input.clone())
                        .with_result(!result.is_error, exec_duration);
                    ctx.tool_result =
                        Some(serde_json::Value::String(result.output.clone()));
                    hooks.execute(HookPoint::PostToolUse, &ctx).await;
                }

                if let Some(ref bus) = config.event_bus {
                    bus.publish(crate::event::OctoEvent::ToolCallCompleted {
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
                    output: result.output.clone(),
                    success: !result.is_error,
                })
                .await;

            let trimmed_output = soft_trim_tool_result(&result.output);

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

            // Record outcome for loop detection
            if let Some(outcome_warning) =
                loop_guard.record_outcome(&tu.name, &input, &result.output)
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

/// Soft-trim tool result if it exceeds the limit (67% head + 27% tail).
fn soft_trim_tool_result(result: &str) -> String {
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
