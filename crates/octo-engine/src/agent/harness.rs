//! Agent Harness — pure-function agent loop entry point.
//!
//! `run_agent_loop(config, messages)` replaces the monolithic `AgentLoop::run()`.
//! All dependencies are injected via `AgentLoopConfig`; the function returns
//! a `BoxStream<AgentEvent>` for fully decoupled consumption.

use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use std::collections::HashMap;
use std::sync::Arc;

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MessageRole, StopReason, StreamEvent, ToolOutput,
};

use crate::security::SafetyDecision;
use crate::tools::approval::{ApprovalDecision, ApprovalGate, ApprovalManager};
use crate::tools::rate_limiter::ToolRateLimiter;

use super::estop::EStopReason;
use super::self_repair::RepairResult;

use crate::context::{
    CompactionContext, ContextPruner, DegradationLevel, MemoryFlusher,
    NewSystemPromptBuilder as SystemPromptBuilder,
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
// use super::CancellationToken;

const TOOL_RESULT_SOFT_LIMIT: usize = 30_000;

/// Maximum tool output size before truncation is applied.
const MAX_TOOL_OUTPUT_SIZE: usize = 100_000;

/// Default context window when no budget is configured (128K tokens).
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

/// Maximum number of retries when the LLM produces a malformed tool call.
const MAX_MALFORMED_TOOL_CALL_RETRIES: u32 = 2;

/// Maximum number of retries when stream consumption fails (JSON parse error, connection drop, etc.)
const MAX_STREAM_ERROR_RETRIES: u32 = 2;

/// Interval (in rounds) at which Zone B working memory is refreshed.
/// This allows agent's memory_edit changes to take effect mid-conversation.
const ZONE_B_REFRESH_INTERVAL: u32 = 5;

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

/// Detect prompt-too-long errors from various LLM providers.
pub(crate) fn is_prompt_too_long(err: &anyhow::Error) -> bool {
    let s = err.to_string().to_lowercase();
    s.contains("prompt_too_long")
        || s.contains("prompt is too long")
        || (s.contains("400") && s.contains("too many tokens"))
        || s.contains("maximum context length")
        || s.contains("context_length_exceeded")
}

/// Maximum number of PTL compact attempts before giving up.
const MAX_COMPACT_ATTEMPTS: u32 = 3;

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
    mut config: AgentLoopConfig,
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

    // Per-tool rate limiter (sliding 60-second window).
    let mut rate_limiter = ToolRateLimiter::new();

    // Autonomous mode state (AQ-T4): initialized from config, tracks tick rounds/budget.
    let mut auto_state: Option<super::autonomous::AutonomousState> = config
        .autonomous
        .as_ref()
        .filter(|c| c.enabled)
        .map(|c| {
            super::autonomous::AutonomousState::new(config.session_id.clone(), c.clone())
        });

    // Per-round memory extractor (AP-D5): incrementally captures memories after each tool round.
    let mut round_memory_extractor = config
        .round_memory_config
        .as_ref()
        .filter(|c| c.enabled)
        .map(|c| crate::memory::round_memory::RoundMemoryExtractor::new(c.clone()));

    // --- SkillSelector: auto-select skill from user message ---
    if config.active_skill.is_none() {
        if let Some(ref skills) = config.skills {
            if !skills.is_empty() {
                // Extract last user message for trigger matching
                let user_msg = messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::User)
                    .and_then(|m| {
                        m.content.iter().find_map(|c| {
                            if let ContentBlock::Text { text } = c {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or("");

                if !user_msg.is_empty() {
                    let trust_mgr = crate::skills::TrustManager::default();
                    let selector = crate::skills::SkillSelector::new(8000, trust_mgr);
                    let selected = selector.select(skills, user_msg);

                    // Pick the highest-scoring non-always skill (always skills are already in the index)
                    if let Some(best) = selected.iter().find(|s| s.score > 0 && s.score < 1000) {
                        if let Some(skill) = skills.iter().find(|s| s.name == best.name) {
                            debug!(skill_name = %skill.name, score = best.score, "SkillSelector: auto-activated skill");
                            config.active_skill = Some(skill.clone());
                        }
                    }
                }
            }
        }
    }

    // --- Model Override: replace model when active skill specifies one ---
    if let Some(ref active_skill) = config.active_skill {
        if let Some(ref skill_model) = active_skill.model {
            if !skill_model.is_empty() {
                debug!(
                    skill = %active_skill.name,
                    model = %skill_model,
                    "Model override from active skill"
                );
                config.model = skill_model.clone();
            }
        }
    }

    // --- Zone A: Build system prompt (static + dynamic separated for prompt caching) ---
    let mut system_prompt = {
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
        // Phase AS: Load CLAUDE.md and other bootstrap files from working directory
        if let Some(ref wd) = config.working_dir {
            builder = builder.with_bootstrap_dir(wd);
        }
        // Phase AS: Inject git status into system prompt (dynamic)
        if let Some(ref git) = config.git_context {
            builder = builder.with_git_status(&git.branch, &git.status, &git.recent_commits);
        }
        // Phase AT: Inject environment info (dynamic)
        let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());
        let os_version = std::env::var("OSTYPE").unwrap_or_else(|_| std::env::consts::OS.to_string());
        builder = builder.with_environment_info(&platform, &shell, &os_version, &config.model);
        // Phase AT: Inject token budget (dynamic)
        builder = builder.with_token_budget(config.max_tokens as usize, 200_000);

        // Use build_separated() for future prompt caching support
        let parts = builder.build_separated();
        let mut prompt = parts.merge();
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

    // --- Zone B+: Cross-session memory injection into system prompt (Phase AG) ---
    // Inject into system_prompt (not user messages) so LLM treats them as background
    // context and does not repeat them in tool results or responses.
    if let Some(ref store) = config.memory_store {
        let first_user_query = messages
            .iter()
            .find(|m| m.role == octo_types::MessageRole::User)
            .map(|m| m.text_content())
            .unwrap_or_default();
        if !first_user_query.is_empty() {
            let injector = crate::memory::MemoryInjector::with_defaults();
            let cross_session = injector
                .build_memory_context(store.as_ref(), config.user_id.as_str(), &first_user_query)
                .await;
            if !cross_session.is_empty() {
                system_prompt.push_str(&cross_session);
                debug!(
                    "System prompt: cross-session memory appended, {} chars",
                    cross_session.len()
                );
            }

            // Phase AS: Pinned high-importance memories (safety net, query-independent)
            let pinned = injector
                .build_pinned_memories(store.as_ref(), config.user_id.as_str(), 0.8, 5, &[])
                .await;
            if !pinned.is_empty() {
                system_prompt.push_str(&pinned);
                debug!("System prompt: pinned memories appended, {} chars", pinned.len());
            }
        }
    }

    // --- Zone B++: Recent session summaries injection (Phase AG) ---
    if let Some(ref summary_store) = config.session_summary_store {
        match summary_store.recent(5).await {
            Ok(summaries) if !summaries.is_empty() => {
                let summary_text = format_session_summaries(&summaries);
                if !summary_text.is_empty() {
                    loop_steps::inject_zone_b(&mut messages, &summary_text);
                    debug!(
                        "Zone B++ injected: {} session summaries, {} chars",
                        summaries.len(),
                        summary_text.len()
                    );
                }
            }
            Err(e) => {
                warn!("Failed to load session summaries: {}", e);
            }
            _ => {}
        }
    }

    // --- Compute tool specs ---
    let tool_specs = tools.specs();

    // --- Context management objects ---
    let mut budget = config.budget.clone().unwrap_or_default();
    let pruner = config.pruner.clone().unwrap_or_default();
    let mut compact_attempts: u32 = 0;
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

    let mut total_tool_calls: u32 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    // Phase AH: Track recent tool names for rich HookContext
    let mut recent_tools: Vec<String> = Vec::new();
    const MAX_RECENT_TOOLS: usize = 10;

    // --- SessionStart hook ---
    if let Some(ref hooks) = config.hook_registry {
        let ctx = build_rich_hook_context(&config, 0, 0, &recent_tools);
        hooks.execute(HookPoint::SessionStart, &ctx).await;
    }

    // P1-1: ContinuationTracker for max_tokens auto-continuation
    let mut continuation_tracker = ContinuationTracker::new(ContinuationConfig {
        max_continuations: config.max_tokens_continuation,
        ..Default::default()
    });

    // AR-T1: TokenEscalation — try upgrading max_tokens before falling back to continuation
    let mut token_escalation = super::token_escalation::TokenEscalation::new();

    // Malformed tool call retry counter — tracks consecutive retries within a turn
    let mut malformed_retry_count: u32 = 0;
    // Stream consumption error counter — retries on JSON parse errors, connection drops, etc.
    let mut stream_error_count: u32 = 0;

    // P1-4: DeferredActionDetector for detecting deferred actions in text
    let deferred_detector = DeferredActionDetector::new();
    let tool_ctx = match config.tool_ctx.clone() {
        Some(ctx) => ctx,
        None => {
            use std::path::PathBuf;
            octo_types::ToolContext {
                sandbox_id: config.sandbox_id.clone(),
                user_id: config.user_id.clone(),
                working_dir: PathBuf::from("."),
                path_validator: None,
            }
        }
    };

    // === Main loop ===
    for round in 0..max_rounds {
        // --- E-Stop check (W10) ---
        if let Some(ref estop) = config.estop {
            if estop.is_triggered() {
                let reason = estop.reason().unwrap_or(EStopReason::SystemShutdown);
                let _ = tx
                    .send(AgentEvent::EmergencyStopped(Some(format!("{}", reason))))
                    .await;
                break;
            }
        }

        // --- Zone B periodic refresh (Phase AG) ---
        // Re-compile working memory every N rounds to reflect updates from memory_edit
        if round > 0 && round % ZONE_B_REFRESH_INTERVAL == 0 {
            if let Some(ref memory) = config.memory {
                let new_xml = memory
                    .compile(&config.user_id, &config.sandbox_id)
                    .await
                    .unwrap_or_default();
                if !new_xml.is_empty() {
                    loop_steps::inject_zone_b(&mut messages, &new_xml);
                    debug!(round, "Zone B refreshed: {} chars", new_xml.len());
                }
            }
        }

        debug!(round, "Harness: round starting");

        // --- Emit IterationStart ---
        let _ = tx.send(AgentEvent::IterationStart { round }).await;

        // --- PreTask hook (round 0 only) ---
        if round == 0 {
            if let Some(ref hooks) = config.hook_registry {
                let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools);
                if let HookAction::Abort(reason) = hooks.execute(HookPoint::PreTask, &ctx).await {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: reason.clone(),
                        })
                        .await;
                    let _ = tx.send(AgentEvent::Done).await;
                    fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
                    return;
                }
            }
        }

        // --- LoopTurnStart hook ---
        if let Some(ref hooks) = config.hook_registry {
            let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools);
            if let HookAction::Abort(reason) = hooks.execute(HookPoint::LoopTurnStart, &ctx).await {
                let _ = tx
                    .send(AgentEvent::Error {
                        message: reason.clone(),
                    })
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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
                let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools)
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

        // --- Canary rotation (W10) ---
        // Rotate the canary token before each LLM call so the safety pipeline
        // checks for the fresh value on output.
        if let Some(ref canary_guard) = config.canary_guard {
            let new_canary = canary_guard.rotate();
            // Replace the canary in the system prompt
            if let Some(start) = system_prompt.rfind("<!-- CANARY: ") {
                if let Some(end) = system_prompt[start..].find(" -->") {
                    let replacement = format!("<!-- CANARY: {} -->", new_canary);
                    system_prompt.replace_range(start..start + end + 4, &replacement);
                }
            }
            debug!("Canary rotated for round {}", round);
        }

        // --- Build CompletionRequest ---
        // P1-2: Apply observation masking when context budget exceeds 50%
        // Below 50% usage, full tool outputs are kept for better LLM reasoning.
        // Above 50%, old tool results are masked to save tokens.
        let usage_ratio = budget.usage_ratio(&system_prompt, &messages, &tool_specs);
        let masked_messages = if usage_ratio > 0.5 {
            let masker = crate::context::ObservationMasker::new(
                crate::context::ObservationMaskConfig {
                    keep_recent_turns: 3,
                    min_mask_length: 200,
                    ..Default::default()
                },
            );
            let masked = masker.mask(&messages);
            debug!(usage_ratio, "ObservationMasker activated (>50% budget)");
            masked
        } else {
            messages.clone()
        };

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

                // PTL recovery — compact and retry instead of terminating
                if is_prompt_too_long(&e) && compact_attempts < MAX_COMPACT_ATTEMPTS {
                    compact_attempts += 1;
                    warn!(
                        attempt = compact_attempts,
                        max = MAX_COMPACT_ATTEMPTS,
                        "prompt_too_long detected, attempting compaction"
                    );

                    // Try LLM-based compaction first (AP-T6 reactive compact)
                    if let Some(ref pipeline) = config.compaction_pipeline {
                        if let Some(ref provider) = config.provider {
                            let ctx = CompactionContext {
                                memory: config.memory.clone(),
                                memory_store: config.memory_store.clone(),
                                active_skill: config.active_skill.clone(),
                                hook_registry: config.hook_registry.clone(),
                                session_summary_store: config.session_summary_store.clone(),
                                user_id: config.user_id.clone(),
                                sandbox_id: config.sandbox_id.clone(),
                                custom_instructions: None,
                            };
                            match pipeline
                                .compact(&messages, provider.as_ref(), &config.model, &ctx)
                                .await
                            {
                                Ok(result) => {
                                    messages.clear();
                                    messages.push(result.boundary_marker);
                                    messages.extend(result.summary_messages);
                                    messages.extend(result.kept_messages);
                                    messages.extend(result.reinjections);

                                    // Append cross-session + pinned memories to system prompt
                                    if !result.system_prompt_additions.is_empty() {
                                        system_prompt.push_str(&result.system_prompt_additions);
                                    }

                                    let _ = tx
                                        .send(AgentEvent::ContextCompacted {
                                            strategy: "llm_summary".into(),
                                            pre_tokens: result.pre_compact_tokens,
                                            post_tokens: result.post_compact_tokens,
                                        })
                                        .await;
                                    // Do NOT trigger Stop hooks — prevents death spiral
                                    continue;
                                }
                                Err(compact_err) => {
                                    warn!("LLM compaction failed: {compact_err}, falling back to truncate");
                                }
                            }
                        }
                    }

                    // Fallback: emergency truncation via pruner
                    let freed = pruner.apply(&mut messages, DegradationLevel::OverflowCompaction);
                    let _ = tx
                        .send(AgentEvent::ContextCompacted {
                            strategy: "truncate_fallback".into(),
                            pre_tokens: 0,
                            post_tokens: freed,
                        })
                        .await;
                    // Do NOT trigger Stop hooks — prevents death spiral
                    continue; // Re-enter loop with truncated messages
                }

                let _ = tx
                    .send(AgentEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
                return;
            }
        };

        // --- Consume stream (P0-5) ---
        let stream_result = consume_stream(&mut llm_stream, &tx, &config.agent_config).await;

        let stream_result = match stream_result {
            Ok(r) => r,
            Err(e) => {
                // Stream consumption failed (JSON parse error, connection drop, etc.)
                // Retry instead of terminating the conversation.
                stream_error_count += 1;
                let err_str = e.to_string();
                if stream_error_count <= MAX_STREAM_ERROR_RETRIES {
                    warn!(
                        attempt = stream_error_count,
                        max = MAX_STREAM_ERROR_RETRIES,
                        "Stream consumption error, retrying: {err_str}"
                    );
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: format!(
                                "Stream error (retry {}/{}): {err_str}",
                                stream_error_count, MAX_STREAM_ERROR_RETRIES
                            ),
                        })
                        .await;
                    // Brief delay before retry
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;
                    continue; // Re-enter loop to retry LLM call
                }
                warn!("Stream error retries exhausted ({MAX_STREAM_ERROR_RETRIES}): {err_str}");
                let _ = tx
                    .send(AgentEvent::Error {
                        message: format!("Stream failed after {MAX_STREAM_ERROR_RETRIES} retries: {err_str}"),
                    })
                    .await;
                let _ = tx.send(AgentEvent::Done).await;
                fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
                return;
            }
        };

        // Reset stream error counter on successful stream consumption
        stream_error_count = 0;

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
            mut tool_uses,
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

        // --- Text tool call recovery (W10) ---
        // Some LLMs emit tool calls as plain text. Attempt to parse them.
        if tool_uses.is_empty() && !full_text.is_empty() {
            let recovered = parse_tool_calls_from_text(&full_text);
            if !recovered.is_empty() {
                debug!(
                    count = recovered.len(),
                    "Recovered tool calls from plain text"
                );
                // Reset malformed retry counter on successful recovery
                malformed_retry_count = 0;
                tool_uses = recovered;
            }
        }

        // --- Malformed / incomplete tool call detection + retry ---
        // If the LLM tried to produce a tool call but it was incomplete or
        // malformed (e.g. truncated XML/JSON), retry instead of silently
        // ending the turn.  This covers both max_tokens truncation and
        // models that emit broken tool call syntax.
        if tool_uses.is_empty() && !full_text.is_empty() {
            let malformed = detect_malformed_tool_call(&full_text);
            if malformed.is_some() && malformed_retry_count < MAX_MALFORMED_TOOL_CALL_RETRIES {
                malformed_retry_count += 1;
                let reason = malformed.unwrap_or_default();

                warn!(
                    attempt = malformed_retry_count,
                    max = MAX_MALFORMED_TOOL_CALL_RETRIES,
                    reason = %reason,
                    "Detected malformed tool call, retrying"
                );

                // Notify user via event
                let _ = tx
                    .send(AgentEvent::RetryingMalformedToolCall {
                        attempt: malformed_retry_count,
                        max_attempts: MAX_MALFORMED_TOOL_CALL_RETRIES,
                        reason: reason.clone(),
                    })
                    .await;

                // Build retry guidance for the model
                let retry_prompt = if stop_reason == StopReason::MaxTokens {
                    format!(
                        "Your previous response was truncated due to the output token limit (stop_reason=max_tokens). \
                         The tool call you were generating was incomplete: {}. \
                         Please re-generate your tool call using the correct structured format. \
                         Keep the tool arguments concise to avoid truncation.",
                        reason
                    )
                } else {
                    format!(
                        "Your previous response contained a malformed tool call that could not be parsed: {}. \
                         Please re-generate your tool call using the correct structured format \
                         (use the tool_use API, not XML or plain text).",
                        reason
                    )
                };

                // Append the broken text as assistant message + retry prompt
                messages.push(ChatMessage::assistant(&full_text));
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: retry_prompt }],
                });
                let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;
                continue; // Re-enter loop for retry
            }
        }

        // --- If no tool uses: check for continuation or finalize ---
        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
            // Reset malformed retry counter on successful non-tool turn
            malformed_retry_count = 0;

            // AR-T1: Try TokenEscalation before ContinuationTracker.
            // If max_tokens caused truncation and we have a higher tier available,
            // upgrade and retry without wasting a continuation round-trip.
            if stop_reason == StopReason::MaxTokens {
                if let Some(new_max) = token_escalation.escalate() {
                    debug!(
                        old = config.max_tokens,
                        new = new_max,
                        "TokenEscalation: upgrading max_tokens"
                    );
                    config.max_tokens = new_max;
                    messages.push(ChatMessage::assistant(&full_text));
                    let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;
                    continue; // Re-enter loop with larger max_tokens
                }
            }

            // P1-1: Auto-continuation on max_tokens (fallback after escalation exhausted)
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
                let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;
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

            let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;

            fire_post_task_hooks(&config, &tx, round, turn_start.elapsed().as_millis() as u64, total_tool_calls, &recent_tools)
                .await;

            // --- Autonomous tick check (AQ-T4): instead of returning, enter tick sleep ---
            if let Some(ref mut state) = auto_state {
                // Budget check
                let budget_reason = state.check_budget();
                if let Some(reason) = budget_reason {
                    let _ = tx.send(AgentEvent::AutonomousExhausted { reason }).await;
                } else {
                    // Record tick + sleep
                    state.record_tick();
                    let sleep_dur = state.effective_sleep_duration();
                    let _ = tx.send(AgentEvent::AutonomousSleeping { duration_secs: sleep_dur }).await;

                    // Interruptible sleep
                    tokio::time::sleep(std::time::Duration::from_secs(sleep_dur)).await;

                    // Check cancellation after sleep
                    if config.cancel_token.is_cancelled() {
                        info!(session = %config.session_id, "Autonomous: cancelled during sleep");
                    } else {
                        let _ = tx.send(AgentEvent::AutonomousTick { round: state.rounds_completed }).await;
                        // Inject tick message and continue main loop
                        let tick_msg = if state.user_online {
                            "<tick> Autonomous check-in. User is online. Summarize progress briefly."
                        } else {
                            "<tick> Autonomous check-in. Continue working quietly."
                        };
                        messages.push(ChatMessage::user(tick_msg));
                        let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;
                        continue; // Re-enter main loop for next LLM call
                    }
                }
            }

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
                    fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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
        // Use the global cancel token so Esc can abort the entire turn,
        // not just the LLM streaming.
        let cancellation_token = config.cancel_token.clone();

        // Build a default ApprovalManager if none was injected (dev mode = auto-approve).
        let approval_mgr = config
            .approval_manager
            .as_ref()
            .map(|m| m.clone())
            .unwrap_or_else(|| Arc::new(ApprovalManager::dev_mode()));

        // --- Phase AS: PermissionEngine pre-check (before ApprovalManager) ---
        // Deny rules block immediately; Ask rules fall through to ApprovalManager.
        let mut permission_blocked = false;
        if let Some(ref perm_engine) = config.permission_engine {
            for (tu, input) in &parsed_tools {
                let decision = perm_engine.evaluate(&tu.name, input);
                match decision {
                    crate::security::permission_types::PermissionDecision::Deny { reason, .. } => {
                        warn!(tool = %tu.name, %reason, "Tool denied by PermissionEngine");
                        let _ = tx
                            .send(AgentEvent::Error {
                                message: format!("Tool '{}' denied by permission policy: {reason}", tu.name),
                            })
                            .await;
                        permission_blocked = true;
                        break;
                    }
                    _ => {
                        // Allow, Ask, UseToolDefault — proceed to ApprovalManager
                    }
                }
            }
        }
        if permission_blocked {
            // Return a tool error so the LLM can retry with different approach
            let error_results: Vec<ContentBlock> = parsed_tools
                .iter()
                .map(|(tu, _)| ContentBlock::ToolResult {
                    tool_use_id: tu.id.clone(),
                    content: "Tool execution denied by permission policy.".to_string(),
                    is_error: true,
                })
                .collect();
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: error_results,
            });
            continue;
        }

        let tool_outputs: Vec<_> = if config.agent_config.enable_parallel {
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
                                fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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
                            fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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

            // --- Rate limit check (parallel): split into allowed / blocked ---
            let mut allowed_indices: Vec<usize> = Vec::new();
            let mut rate_limited_results: HashMap<usize, ToolOutput> = HashMap::new();
            for (i, (tu, _input)) in parsed_tools.iter().enumerate() {
                if let Some(tool) = tools.get(&tu.name) {
                    let limit = tool.rate_limit();
                    if limit > 0 && !rate_limiter.check_and_record(&tu.name, limit) {
                        warn!(tool = %tu.name, limit, "Tool rate limit exceeded (parallel)");
                        rate_limited_results.insert(
                            i,
                            ToolOutput::error(format!(
                                "Rate limit exceeded for tool '{}': max {} calls per 60s",
                                tu.name, limit
                            )),
                        );
                        continue;
                    }
                }
                allowed_indices.push(i);
            }

            let tools_to_run: Vec<_> = allowed_indices
                .iter()
                .map(|&i| {
                    let (tu, input) = &parsed_tools[i];
                    (tu.name.clone(), input.clone())
                })
                .collect();

            let config_timeout = if config.tool_timeout_secs > 0 {
                Some(config.tool_timeout_secs)
            } else {
                None
            };
            let parallel_results = execute_parallel(
                tools_to_run,
                &tools,
                config.agent_config.max_parallel_tools,
                &cancellation_token,
                &tool_ctx,
                config_timeout,
            )
            .await;

            // Merge parallel results back with rate-limited error results.
            let mut parallel_iter = parallel_results.into_iter();
            let mut merged: Vec<(String, ToolOutput)> = Vec::with_capacity(parsed_tools.len());
            for i in 0..parsed_tools.len() {
                if let Some(err_result) = rate_limited_results.remove(&i) {
                    merged.push((parsed_tools[i].0.name.clone(), err_result));
                } else {
                    if let Some(r) = parallel_iter.next() {
                        merged.push(r);
                    }
                }
            }

            parsed_tools
                .iter()
                .zip(merged)
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
                                fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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
                            fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
                            return;
                        }
                        ApprovalDecision::Approved => {
                            debug!(tool = %tu.name, "Tool auto-approved");
                        }
                    }
                }

                // PreToolUse hook
                if let Some(ref hooks) = config.hook_registry {
                    let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools)
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

                // --- Rate limit check (sequential) ---
                if let Some(tool) = tools.get(&tu.name) {
                    let limit = tool.rate_limit();
                    if limit > 0 && !rate_limiter.check_and_record(&tu.name, limit) {
                        warn!(tool = %tu.name, limit, "Tool rate limit exceeded");
                        let result = ToolOutput::error(format!(
                            "Rate limit exceeded for tool '{}': max {} calls per 60s",
                            tu.name, limit
                        ));
                        outputs.push((tu, input.clone(), result));
                        continue;
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
                    let mut ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools)
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
        let mut blob_replacements: Vec<(usize, String)> = Vec::new();
        for (tu, input, result) in tool_outputs {
            total_tool_calls += 1;

            // Phase AH: Track recent tool names for rich HookContext
            recent_tools.push(tu.name.clone());
            if recent_tools.len() > MAX_RECENT_TOOLS {
                recent_tools.remove(0);
            }

            // Record tool execution to SQLite for observability
            if let Some(ref recorder) = config.recorder {
                let source = octo_types::ToolSource::BuiltIn;
                let input_val = input.clone();
                match recorder
                    .record_start(
                        config.session_id.as_str(),
                        config.user_id.as_str(),
                        &tu.name,
                        &source,
                        &input_val,
                    )
                    .await
                {
                    Ok(exec_id) => {
                        let output_val = serde_json::Value::String(result.content.clone());
                        let duration = result.duration_ms;
                        if result.is_error {
                            let _ = recorder
                                .record_failed(&exec_id, &result.content, duration)
                                .await;
                        } else {
                            let _ = recorder
                                .record_complete(&exec_id, &output_val, duration)
                                .await;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, tool = %tu.name, "Failed to record tool execution");
                    }
                }
            }

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

            // --- Self-Repair check (W10) ---
            if let Some(ref mut self_repair) = config.self_repair {
                let repair = self_repair.check_and_repair(&tu.name, result.is_error);
                match repair {
                    RepairResult::Repaired(hint) => {
                        debug!(tool = %tu.name, "Self-repair: injecting hint");
                        // Add hint as a system message for next LLM call
                        messages.push(ChatMessage {
                            role: MessageRole::User,
                            content: vec![ContentBlock::Text { text: hint }],
                        });
                    }
                    RepairResult::Unrecoverable { reason } => {
                        let _ = tx
                            .send(AgentEvent::Error {
                                message: format!("Tool {} stuck, unrecoverable: {}", tu.name, reason),
                            })
                            .await;
                        let _ = tx.send(AgentEvent::Done).await;
                        fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
                        return;
                    }
                    _ => {}
                }
            }

            // --- BlobStore: externalize large tool results (AQ-T3) ---
            // Current round: LLM sees full output (trimmed_output) so it can reason.
            // After push to messages: replace with blob reference to save context tokens
            // on future rounds and session reloads.
            let blob_ref = if let Some(ref blob_store) = config.blob_store {
                if trimmed_output.len() > crate::storage::blob_store::BLOB_THRESHOLD_BYTES {
                    match blob_store.store(trimmed_output.as_bytes()) {
                        Ok(hash) => {
                            debug!(
                                tool = %tu.name,
                                hash = %hash,
                                original_size = trimmed_output.len(),
                                "Tool output externalized to blob store"
                            );
                            Some(crate::storage::blob_store::BlobStore::format_blob_ref(&hash))
                        }
                        Err(e) => {
                            warn!(tool = %tu.name, error = %e, "Failed to store blob, keeping inline");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: tu.id.clone(),
                content: trimmed_output,  // LLM sees full content this round
                is_error: result.is_error,
            });
            // Track which tool_results indices need blob replacement after LLM sees them
            if let Some(ref_str) = blob_ref {
                blob_replacements.push((tool_results.len() - 1, ref_str));
            }
        }

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: tool_results,
        });

        // Replace large tool outputs with blob references in the persisted history.
        // The LLM has already seen the full content above; subsequent rounds and
        // session reloads will see compact blob refs instead.
        if !blob_replacements.is_empty() {
            if let Some(last_msg) = messages.last_mut() {
                for (idx, ref_str) in blob_replacements.drain(..) {
                    if let Some(ContentBlock::ToolResult { content, .. }) = last_msg.content.get_mut(idx) {
                        *content = ref_str;
                    }
                }
            }
        }

        // --- Per-round memory extraction (AP-D5) ---
        if let (Some(ref mut extractor), Some(ref store)) =
            (&mut round_memory_extractor, &config.memory_store)
        {
            let stored = extractor
                .extract_round(&messages, store.as_ref(), config.user_id.as_str(), round)
                .await;
            if stored > 0 {
                let _ = tx
                    .send(AgentEvent::MemoryFlushed {
                        facts_count: stored,
                    })
                    .await;
            }
        }

        // --- IterationEnd + LoopTurnEnd hook ---
        let _ = tx.send(AgentEvent::IterationEnd { round, input_tokens: total_input_tokens, output_tokens: total_output_tokens }).await;

        if let Some(ref hooks) = config.hook_registry {
            let elapsed = turn_start.elapsed().as_millis() as u64;
            let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools)
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
    fire_session_end(&config, &tx, total_tool_calls, &recent_tools).await;
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
async fn fire_session_end(
    config: &AgentLoopConfig,
    _tx: &mpsc::Sender<AgentEvent>,
    total_tool_calls: u32,
    recent_tools: &[String],
) {
    if let Some(ref hooks) = config.hook_registry {
        let ctx = build_rich_hook_context(config, 0, total_tool_calls, recent_tools);
        hooks.execute(HookPoint::SessionEnd, &ctx).await;
    }
}

/// Fire PostTask + LoopTurnEnd + SessionEnd hooks at end of successful turn.
async fn fire_post_task_hooks(
    config: &AgentLoopConfig,
    tx: &mpsc::Sender<AgentEvent>,
    round: u32,
    elapsed_ms: u64,
    total_tool_calls: u32,
    recent_tools: &[String],
) {
    if let Some(ref hooks) = config.hook_registry {
        let ctx = build_rich_hook_context(config, round, total_tool_calls, recent_tools)
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

/// Build a rich HookContext from the current agent loop state (Phase AH).
///
/// Populates environment, history, and session fields from `AgentLoopConfig`.
fn build_rich_hook_context(
    config: &AgentLoopConfig,
    round: u32,
    total_tool_calls: u32,
    recent_tools: &[String],
) -> HookContext {
    let mut ctx = HookContext::new()
        .with_session(config.session_id.as_str())
        .with_turn(round)
        .with_history(total_tool_calls, round, recent_tools.to_vec());

    // Environment context from config
    if let Some(ref tool_ctx) = config.tool_ctx {
        ctx.working_dir = Some(tool_ctx.working_dir.display().to_string());
    }
    ctx.model = Some(config.model.clone());

    // Agent ID from manifest
    if let Some(ref manifest) = config.manifest {
        ctx.agent_id = Some(manifest.name.clone());
    }

    // Active skill name
    if let Some(ref skill) = config.active_skill {
        ctx.skill_name = Some(skill.name.clone());
    }

    ctx
}

// ---------------------------------------------------------------------------
// Malformed / incomplete tool call detection
// ---------------------------------------------------------------------------
// Detects when the LLM attempted to produce a tool call but the output was
// incomplete or malformed (e.g. truncated by max_tokens).  Returns a
// human-readable reason string if a malformed tool call pattern is found.
// This is intentionally conservative — only matches clear indicators of
// attempted-but-broken tool invocations, not arbitrary text.
// ---------------------------------------------------------------------------

/// Detect malformed or incomplete tool call patterns in LLM text output.
///
/// Returns `Some(reason)` if the text contains evidence of an attempted
/// but incomplete tool call, or `None` if the text looks like normal prose.
fn detect_malformed_tool_call(text: &str) -> Option<String> {
    // Strategy 1: Qwen-style XML — opening <tool_call> or <function= without proper closing
    if text.contains("<tool_call>") || text.contains("<function=") {
        // Check if there's a properly closed tool call (already handled by parse_tool_calls_from_text)
        let has_complete = text.contains("</function>") && text.contains("</tool_call>");
        if !has_complete {
            return Some("Incomplete XML tool call (truncated <tool_call>/<function=> block)".into());
        }
    }

    // Strategy 2: JSON tool call — opening {"name": ... without proper closing
    // Look for patterns like {"name": "bash", "arguments": { ... without closing }}
    if let Some(start) = text.find(r#""name""#) {
        let sub = &text[start..];
        if sub.contains(r#""arguments""#) {
            // Check if the JSON object is properly closed
            let open_braces = sub.chars().filter(|&c| c == '{').count();
            let close_braces = sub.chars().filter(|&c| c == '}').count();
            if open_braces > close_braces {
                return Some("Incomplete JSON tool call (unclosed braces in name/arguments object)".into());
            }
        }
    }

    // Strategy 3: Fenced JSON block opened but not closed
    if text.contains("```json") && !text.contains("```\n") && text.matches("```").count() == 1 {
        // Only one ``` found after ```json — the block was never closed
        if text.contains(r#""name""#) && text.contains(r#""arguments""#) {
            return Some("Incomplete fenced JSON tool call (unclosed code block)".into());
        }
    }

    None
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

    // --- Strategy 2: XML format (simple: <tool_name>{json}</tool_name>) ---
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

    // --- Strategy 3: Qwen-style XML (<tool_call><function=name><parameter=key>value</parameter></function></tool_call>) ---
    if results.is_empty() {
        static RE_QWEN: OnceLock<Regex> = OnceLock::new();
        let re_qwen = RE_QWEN.get_or_init(|| {
            Regex::new(r"(?s)<tool_call>\s*<function=([a-zA-Z_][a-zA-Z0-9_-]*)>(.*?)</function>\s*</tool_call>")
                .expect("valid regex")
        });
        static RE_PARAM: OnceLock<Regex> = OnceLock::new();
        let re_param = RE_PARAM.get_or_init(|| {
            Regex::new(r"(?s)<parameter=([a-zA-Z_][a-zA-Z0-9_-]*)>(.*?)</parameter>")
                .expect("valid regex")
        });

        for cap in re_qwen.captures_iter(text) {
            if let (Some(func_name), Some(body)) = (cap.get(1), cap.get(2)) {
                let name = func_name.as_str().to_string();
                let mut args = serde_json::Map::new();
                for param_cap in re_param.captures_iter(body.as_str()) {
                    if let (Some(key), Some(val)) = (param_cap.get(1), param_cap.get(2)) {
                        args.insert(
                            key.as_str().to_string(),
                            serde_json::Value::String(val.as_str().trim().to_string()),
                        );
                    }
                }
                if !args.is_empty() {
                    results.push(PendingToolUse {
                        id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                        name,
                        input_json: serde_json::Value::Object(args).to_string(),
                    });
                }
            }
        }
    }

    results
}

/// Format recent session summaries into a context block for Zone B injection.
///
/// Budget: max 2000 chars total. Oldest summaries are dropped first if over budget.
fn format_session_summaries(summaries: &[crate::memory::SessionSummary]) -> String {
    const MAX_CHARS: usize = 2000;

    let mut lines = Vec::new();
    lines.push("## Recent Sessions".to_string());

    for s in summaries {
        let date = chrono::DateTime::from_timestamp(s.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let topics = if s.key_topics.is_empty() {
            String::new()
        } else {
            format!(" [{}]", s.key_topics.join(", "))
        };

        lines.push(format!("- [{}] {}{}", date, s.summary, topics));
    }

    let result = lines.join("\n");
    if result.len() <= MAX_CHARS {
        result
    } else {
        // Drop oldest summaries (last in list since they're ordered desc) until under budget
        while lines.len() > 2 {
            // Keep header + at least 1 entry
            let total: usize = lines.iter().map(|l| l.len() + 1).sum();
            if total <= MAX_CHARS {
                break;
            }
            lines.pop();
        }
        lines.join("\n")
    }
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

    #[test]
    fn test_parse_tool_calls_qwen_xml_format() {
        let text = r#"Let me run that.
<tool_call>
<function=bash>
<parameter=command>python3 -c "print('hello')"</parameter>
</function>
</tool_call>"#;
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        let args: serde_json::Value = serde_json::from_str(&calls[0].input_json).unwrap();
        assert_eq!(args["command"], "python3 -c \"print('hello')\"");
    }

    #[test]
    fn test_parse_tool_calls_qwen_multiple_params() {
        let text = r#"<tool_call>
<function=file_write>
<parameter=path>/tmp/test.txt</parameter>
<parameter=content>Hello World</parameter>
</function>
</tool_call>"#;
        let calls = parse_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "file_write");
        let args: serde_json::Value = serde_json::from_str(&calls[0].input_json).unwrap();
        assert_eq!(args["path"], "/tmp/test.txt");
        assert_eq!(args["content"], "Hello World");
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

    // --- Malformed tool call detection tests ---

    #[test]
    fn test_detect_malformed_qwen_xml_truncated() {
        // Real-world case: Qwen model output truncated mid-XML
        let text = r#"<tool_call>
<function=bash>
<parameter=command>cd /tmp && nohup node server.js &</parameter>
</function"#;
        let result = detect_malformed_tool_call(text);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Incomplete XML"));
    }

    #[test]
    fn test_detect_malformed_qwen_xml_no_close_tag() {
        let text = "<tool_call>\n<function=bash>\n<parameter=command>ls -la</parameter>";
        let result = detect_malformed_tool_call(text);
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_malformed_json_unclosed_braces() {
        let text = r#"I'll run this: {"name": "bash", "arguments": {"command": "echo hello"#;
        let result = detect_malformed_tool_call(text);
        assert!(result.is_some());
        assert!(result.unwrap().contains("unclosed braces"));
    }

    #[test]
    fn test_detect_malformed_fenced_json_unclosed() {
        // This text has unclosed JSON braces AND unclosed fenced block.
        // Strategy 2 (JSON unclosed braces) fires first since it comes before Strategy 3.
        let text = "Sure, let me do that.\n```json\n{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"";
        let result = detect_malformed_tool_call(text);
        assert!(result.is_some());
        assert!(result.unwrap().contains("unclosed braces"));
    }

    #[test]
    fn test_detect_malformed_normal_text_returns_none() {
        let text = "Here's the answer to your question. The function works correctly.";
        assert!(detect_malformed_tool_call(text).is_none());
    }

    #[test]
    fn test_detect_malformed_complete_qwen_returns_none() {
        // Complete Qwen XML should NOT trigger malformed detection
        let text = r#"<tool_call>
<function=bash>
<parameter=command>ls -la</parameter>
</function>
</tool_call>"#;
        assert!(detect_malformed_tool_call(text).is_none());
    }

    #[test]
    fn test_detect_malformed_complete_json_returns_none() {
        // Complete JSON should NOT trigger (it will be handled by parse_tool_calls_from_text)
        let text = r#"{"name": "bash", "arguments": {"command": "ls"}}"#;
        assert!(detect_malformed_tool_call(text).is_none());
    }
}

// ---------------------------------------------------------------------------
// AR-T4: Conversation rewind helper
// ---------------------------------------------------------------------------

/// Rewind messages to the specified turn.
///
/// A "turn" is a (user, assistant) pair. Turn 0 = the first complete pair.
/// System messages are always preserved. Messages after `to_turn` are dropped.
pub fn rewind_messages(messages: &mut Vec<ChatMessage>, to_turn: usize) {
    // Count turns (assistant responses) and find the cutoff point.
    let mut turn_count = 0;
    let mut keep_until = 0;

    for (i, msg) in messages.iter().enumerate() {
        keep_until = i + 1;
        if msg.role == MessageRole::Assistant {
            if turn_count >= to_turn {
                break;
            }
            turn_count += 1;
        }
    }

    messages.truncate(keep_until);
}

#[cfg(test)]
mod rewind_tests {
    use super::*;

    fn make_messages(pairs: usize) -> Vec<ChatMessage> {
        let mut msgs = Vec::new();
        for i in 0..pairs {
            msgs.push(ChatMessage::user(format!("Question {}", i)));
            msgs.push(ChatMessage::assistant(format!("Answer {}", i)));
        }
        msgs
    }

    #[test]
    fn test_rewind_to_first_turn() {
        let mut msgs = make_messages(5);
        assert_eq!(msgs.len(), 10); // 5*(user+assistant)
        rewind_messages(&mut msgs, 0);
        // Should keep: user0, assistant0
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_rewind_to_middle() {
        let mut msgs = make_messages(5);
        rewind_messages(&mut msgs, 2);
        // 3 pairs (turns 0, 1, 2) = 6 messages
        assert_eq!(msgs.len(), 6);
    }

    #[test]
    fn test_rewind_beyond_end_keeps_all() {
        let mut msgs = make_messages(3);
        let original_len = msgs.len();
        rewind_messages(&mut msgs, 10);
        assert_eq!(msgs.len(), original_len);
    }

    #[test]
    fn test_rewind_empty() {
        let mut msgs: Vec<ChatMessage> = Vec::new();
        rewind_messages(&mut msgs, 0);
        assert!(msgs.is_empty());
    }
}
