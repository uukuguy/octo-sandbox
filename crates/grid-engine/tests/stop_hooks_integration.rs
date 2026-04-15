//! S3.T4 — Integration tests for Stop hooks wired into `run_agent_loop`.
//!
//! Tests use a `MockProvider` (no live LLM) modelled after
//! `d87_multi_step_workflow_regression.rs`. Each test exercises the full
//! `run_agent_loop_inner` body:
//!
//!   1. `test_stop_hook_noop_unchanged` — Noop hook → loop terminates normally
//!   2. `test_stop_hook_inject_and_continue` — InjectAndContinue once → loop
//!      runs an extra round
//!   3. `test_stop_hook_skipped_on_api_error` — provider Err on first call →
//!      hook NEVER fires (death-spiral prevention)
//!   4. `test_stop_hook_max_injection_cap` — hook always injects → loop stops
//!      after `MAX_STOP_HOOK_INJECTIONS` and commits the final response

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};

use grid_engine::agent::stop_hooks::{
    StopHook, StopHookDecision, MAX_STOP_HOOK_INJECTIONS,
};
use grid_engine::agent::{run_agent_loop, AgentConfig, AgentEvent, AgentLoopConfig};
use grid_engine::hooks::HookContext;
use grid_engine::providers::{CompletionStream, Provider};
use grid_engine::tools::ToolRegistry;
use grid_types::{
    ChatMessage, CompletionRequest, CompletionResponse, StopReason, StreamEvent, TokenUsage,
};

// ---------------------------------------------------------------------------
// Mock provider — emits text-only EndTurn responses (natural termination)
// ---------------------------------------------------------------------------

/// Provider that always returns text-only `EndTurn` so the loop hits the
/// stop-hook boundary on every call. `call_count` lets tests observe how
/// many rounds the loop executed.
struct TextOnlyProvider {
    call_count: Arc<AtomicU32>,
}

impl TextOnlyProvider {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }

    fn count(&self) -> Arc<AtomicU32> {
        self.call_count.clone()
    }

    fn final_text_stream(text: &str) -> CompletionStream {
        let events: Vec<Result<StreamEvent>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg_text".into(),
            }),
            Ok(StreamEvent::TextDelta {
                text: text.to_string(),
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 50,
                    output_tokens: 30,
                },
            }),
        ];
        Box::pin(stream::iter(events))
    }
}

#[async_trait]
impl Provider for TextOnlyProvider {
    fn id(&self) -> &str {
        "text-only-mock"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        unimplemented!("streaming only")
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(Self::final_text_stream(&format!("response {}", n)))
    }
}

/// Provider that always returns Err on `stream()` — used for the
/// API-error path test.
struct ErrorProvider;

#[async_trait]
impl Provider for ErrorProvider {
    fn id(&self) -> &str {
        "error-mock"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        unimplemented!()
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("simulated upstream failure"))
    }
}

// ---------------------------------------------------------------------------
// Mock stop hooks
// ---------------------------------------------------------------------------

/// Records that it ran but otherwise lets the loop terminate.
struct CountingNoopHook {
    ran: Arc<AtomicU32>,
}

#[async_trait]
impl StopHook for CountingNoopHook {
    fn name(&self) -> &str {
        "counting-noop"
    }
    async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
        self.ran.fetch_add(1, Ordering::SeqCst);
        Ok(StopHookDecision::Noop)
    }
}

/// Returns `InjectAndContinue` once, then `Noop` for all subsequent calls.
/// Lets us assert that the loop ran exactly one extra round.
struct InjectOnceHook {
    ran: Arc<AtomicU32>,
}

#[async_trait]
impl StopHook for InjectOnceHook {
    fn name(&self) -> &str {
        "inject-once"
    }
    async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
        let n = self.ran.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            Ok(StopHookDecision::InjectAndContinue(vec![ChatMessage::user(
                "please continue the workflow",
            )]))
        } else {
            Ok(StopHookDecision::Noop)
        }
    }
}

/// Always returns `InjectAndContinue` — exercises the cap.
struct AlwaysInjectHook {
    ran: Arc<AtomicU32>,
}

#[async_trait]
impl StopHook for AlwaysInjectHook {
    fn name(&self) -> &str {
        "always-inject"
    }
    async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
        self.ran.fetch_add(1, Ordering::SeqCst);
        Ok(StopHookDecision::InjectAndContinue(vec![ChatMessage::user(
            "again",
        )]))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn empty_tools() -> Arc<ToolRegistry> {
    Arc::new(ToolRegistry::new())
}

fn quiet_agent_config() -> AgentConfig {
    AgentConfig {
        enable_typing_signal: false,
        enable_parallel: false,
        ..AgentConfig::default()
    }
}

/// 1. Noop hook: loop terminates normally after a single LLM call.
#[tokio::test]
async fn test_stop_hook_noop_unchanged() {
    let provider = Arc::new(TextOnlyProvider::new());
    let provider_count = provider.count();

    let hook_runs = Arc::new(AtomicU32::new(0));
    let hook = Arc::new(CountingNoopHook {
        ran: hook_runs.clone(),
    });

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(empty_tools())
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(5)
        .force_text_at_last(false)
        .agent_config(quiet_agent_config())
        .stop_hook(hook)
        .build();

    let messages = vec![ChatMessage::user("hello")];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    let completed_rounds = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Completed(r) => Some(r.rounds),
            _ => None,
        })
        .expect("expected Completed event");

    assert_eq!(
        completed_rounds, 1,
        "Noop hook should not extend the loop; expected 1 round, got {}",
        completed_rounds
    );
    assert_eq!(
        provider_count.load(Ordering::SeqCst),
        1,
        "provider should have been called exactly once"
    );
    assert_eq!(
        hook_runs.load(Ordering::SeqCst),
        1,
        "hook should have run exactly once at the natural termination boundary"
    );
}

/// 2. InjectAndContinue once: loop runs an extra round, second call sees the
/// injected user message in conversation history.
#[tokio::test]
async fn test_stop_hook_inject_and_continue() {
    let provider = Arc::new(TextOnlyProvider::new());
    let provider_count = provider.count();

    let hook_runs = Arc::new(AtomicU32::new(0));
    let hook = Arc::new(InjectOnceHook {
        ran: hook_runs.clone(),
    });

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(empty_tools())
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(5)
        .force_text_at_last(false)
        .agent_config(quiet_agent_config())
        .stop_hook(hook)
        .build();

    let messages = vec![ChatMessage::user("start")];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    let completed = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Completed(r) => Some(r.clone()),
            _ => None,
        })
        .expect("expected Completed event");

    // Round indexing is 0-based internally but `Completed.rounds` is `round + 1`,
    // so two rounds means round=1 at termination → rounds field == 2.
    assert_eq!(
        completed.rounds, 2,
        "InjectAndContinue should produce one re-entry; expected 2 rounds, got {}",
        completed.rounds
    );
    assert_eq!(
        provider_count.load(Ordering::SeqCst),
        2,
        "provider should have been called twice (initial + post-injection)"
    );
    assert_eq!(
        hook_runs.load(Ordering::SeqCst),
        2,
        "hook should have run after each round (first inject, second noop)"
    );

    // Final message history must contain the injected user prompt.
    let injected_present = completed
        .final_messages
        .iter()
        .any(|m| m.text_content().contains("please continue the workflow"));
    assert!(
        injected_present,
        "injected user message must land in conversation history before termination"
    );
}

/// 3. Provider error before stop boundary: hook NEVER fires. The error path
/// returns directly from `run_agent_loop_inner` without touching the stop-hook
/// dispatch site (death-spiral prevention).
#[tokio::test]
async fn test_stop_hook_skipped_on_api_error() {
    let provider = Arc::new(ErrorProvider);

    let hook_runs = Arc::new(AtomicU32::new(0));
    let hook = Arc::new(CountingNoopHook {
        ran: hook_runs.clone(),
    });

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(empty_tools())
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(5)
        .force_text_at_last(false)
        .agent_config(quiet_agent_config())
        .stop_hook(hook)
        .build();

    let messages = vec![ChatMessage::user("trigger error")];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // Should see Error + Done, no Completed.
    let saw_error = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Error { .. }));
    let saw_done = events.iter().any(|e| matches!(e, AgentEvent::Done));
    let saw_completed = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Completed(_)));

    assert!(saw_error, "expected AgentEvent::Error from upstream failure");
    assert!(saw_done, "expected AgentEvent::Done after error");
    assert!(
        !saw_completed,
        "Completed must NOT fire when provider errored"
    );

    assert_eq!(
        hook_runs.load(Ordering::SeqCst),
        0,
        "Stop hook MUST NOT fire on API error path (death-spiral prevention)"
    );
}

/// 4. Always-inject hook: loop terminates after `MAX_STOP_HOOK_INJECTIONS`
/// re-entries even though the hook never returns Noop. The provider should
/// be called `MAX_STOP_HOOK_INJECTIONS + 1` times (initial + N re-entries).
#[tokio::test]
async fn test_stop_hook_max_injection_cap() {
    let provider = Arc::new(TextOnlyProvider::new());
    let provider_count = provider.count();

    let hook_runs = Arc::new(AtomicU32::new(0));
    let hook = Arc::new(AlwaysInjectHook {
        ran: hook_runs.clone(),
    });

    // max_iterations must comfortably exceed MAX_STOP_HOOK_INJECTIONS so that
    // the cap (not the iteration limit) is what stops the loop.
    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(empty_tools())
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations((MAX_STOP_HOOK_INJECTIONS + 5) as u32)
        .force_text_at_last(false)
        .agent_config(quiet_agent_config())
        .stop_hook(hook)
        .build();

    let messages = vec![ChatMessage::user("loop")];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    let completed = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::Completed(r) => Some(r.clone()),
            _ => None,
        })
        .expect("expected Completed event after cap hit");

    // Initial round + N injection rounds = N+1 provider calls and N+1 rounds.
    let expected_rounds = MAX_STOP_HOOK_INJECTIONS + 1;
    assert_eq!(
        completed.rounds, expected_rounds,
        "loop must stop at MAX_STOP_HOOK_INJECTIONS+1 rounds; expected {}, got {}",
        expected_rounds, completed.rounds
    );
    assert_eq!(
        provider_count.load(Ordering::SeqCst),
        expected_rounds,
        "provider call count should equal MAX_STOP_HOOK_INJECTIONS+1"
    );
    // Hook fires once per round (every termination boundary).
    assert_eq!(
        hook_runs.load(Ordering::SeqCst),
        expected_rounds,
        "hook should run on every termination boundary including the capped one"
    );
}
