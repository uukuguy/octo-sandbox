//! Phase 4a T1 / D151 — harness envelope **call-site** wiring regression.
//!
//! Phase 3.6 T1 wired `.with_event("PreToolUse" | "PostToolUse" | "Stop")` at
//! three dispatch sites in `crates/grid-engine/src/agent/harness.rs`
//! (L1766 Stop / L2236 PreToolUse / L2390 PostToolUse). The serializer-level
//! tests in `hook_envelope_parity_test.rs` already lock the `HookContext →
//! JSON` projection, but they construct the context themselves — so if a
//! future refactor dropped `.with_event(...)` at one of the dispatch sites,
//! no existing test in the repo would catch it (the D136 xfail mask on
//! `test_hook_envelope.py --runtime=grid` hides the Pre/Post regression).
//!
//! This file fills that gap by running the real `run_agent_loop` with spy
//! [`HookHandler`] / [`StopHook`] impls that capture `ctx.event.clone()` and
//! asserting the ADR-V2-006 literal strings surface at each dispatch point.
//!
//! **Revert validation**: mentally (or temporarily) deleting the
//! `.with_event("PreToolUse")` / `.with_event("PostToolUse")` /
//! `.with_event("Stop")` call at the respective harness site will make
//! the corresponding test observe `ctx.event == None` and fail its
//! `assert_eq!(..., "PreToolUse" | "PostToolUse" | "Stop")` assertion.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde_json::json;

use grid_engine::agent::stop_hooks::{StopHook, StopHookDecision};
use grid_engine::agent::{run_agent_loop, AgentConfig, AgentEvent, AgentLoopConfig};
use grid_engine::hooks::{HookAction, HookContext, HookHandler, HookPoint, HookRegistry};
use grid_engine::providers::{CompletionStream, Provider};
use grid_engine::tools::{Tool, ToolRegistry};
use grid_types::{
    ChatMessage, CompletionRequest, CompletionResponse, StopReason, StreamEvent, TokenUsage,
    ToolContext, ToolOutput, ToolSource,
};

// ---------------------------------------------------------------------------
// Spy HookHandler / StopHook — capture `ctx.event` into a shared Vec
// ---------------------------------------------------------------------------

struct EventSpyHandler {
    name: &'static str,
    captured: Arc<Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl HookHandler for EventSpyHandler {
    fn name(&self) -> &str {
        self.name
    }

    async fn execute(&self, ctx: &HookContext) -> Result<HookAction> {
        self.captured.lock().unwrap().push(ctx.event.clone());
        Ok(HookAction::Continue)
    }
}

struct EventSpyStopHook {
    captured: Arc<Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl StopHook for EventSpyStopHook {
    fn name(&self) -> &str {
        "stop-event-spy"
    }

    async fn execute(&self, ctx: &HookContext) -> Result<StopHookDecision> {
        self.captured.lock().unwrap().push(ctx.event.clone());
        Ok(StopHookDecision::Noop)
    }
}

// ---------------------------------------------------------------------------
// Mock provider — one tool-use round then final text (drives Pre + Post + Stop)
// ---------------------------------------------------------------------------

struct OneToolThenFinal {
    call_count: AtomicU32,
}

impl OneToolThenFinal {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
        }
    }

    fn tool_use_stream() -> CompletionStream {
        let events: Vec<Result<StreamEvent>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg_tool".into(),
            }),
            Ok(StreamEvent::ToolUseComplete {
                index: 0,
                id: "toolu_wire_spy".into(),
                name: "noop_tool".into(),
                input: json!({}),
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 10,
                },
            }),
        ];
        Box::pin(stream::iter(events))
    }

    fn final_text_stream() -> CompletionStream {
        let events: Vec<Result<StreamEvent>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg_final".into(),
            }),
            Ok(StreamEvent::TextDelta {
                text: "done".into(),
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            }),
        ];
        Box::pin(stream::iter(events))
    }
}

#[async_trait]
impl Provider for OneToolThenFinal {
    fn id(&self) -> &str {
        "one-tool-then-final"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        unimplemented!("streaming only")
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            Ok(Self::tool_use_stream())
        } else {
            Ok(Self::final_text_stream())
        }
    }
}

// ---------------------------------------------------------------------------
// Stub tool — executes successfully with no side effects
// ---------------------------------------------------------------------------

struct NoopTool;

#[async_trait]
impl Tool for NoopTool {
    fn name(&self) -> &str {
        "noop_tool"
    }
    fn description(&self) -> &str {
        "no-op tool for envelope wiring tests"
    }
    fn parameters(&self) -> serde_json::Value {
        json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::success("ok".to_string()))
    }
    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

// ---------------------------------------------------------------------------
// Test harness helper — build an AgentLoop with registered spies
// ---------------------------------------------------------------------------

struct Spies {
    pre: Arc<Mutex<Vec<Option<String>>>>,
    post: Arc<Mutex<Vec<Option<String>>>>,
    stop: Arc<Mutex<Vec<Option<String>>>>,
}

/// Build a minimal AgentLoop, register spies for PreToolUse / PostToolUse
/// handler slots + a StopHook slot, run one loop that executes one tool
/// call then terminates, and return the captured event vectors.
async fn run_with_spies() -> Spies {
    let pre = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
    let post = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
    let stop = Arc::new(Mutex::new(Vec::<Option<String>>::new()));

    let registry = HookRegistry::new();
    registry
        .register(
            HookPoint::PreToolUse,
            Arc::new(EventSpyHandler {
                name: "pre-event-spy",
                captured: pre.clone(),
            }),
        )
        .await;
    registry
        .register(
            HookPoint::PostToolUse,
            Arc::new(EventSpyHandler {
                name: "post-event-spy",
                captured: post.clone(),
            }),
        )
        .await;
    let hook_registry = Arc::new(registry);

    let stop_hook = Arc::new(EventSpyStopHook {
        captured: stop.clone(),
    });

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(NoopTool);
    let tool_registry = Arc::new(tool_registry);

    let provider = Arc::new(OneToolThenFinal::new());

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(tool_registry)
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(5)
        .force_text_at_last(false)
        .hook_registry(hook_registry)
        .stop_hook(stop_hook)
        .agent_config(AgentConfig {
            enable_typing_signal: false,
            enable_parallel: false,
            ..AgentConfig::default()
        })
        .build();

    let messages = vec![ChatMessage::user("run the wiring spy workflow")];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // Sanity: the loop must have actually reached Completed, otherwise the
    // spy captures are meaningless. Assert here so a broken test harness
    // surfaces before the per-event asserts.
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Completed(_))),
        "fixture failure: agent loop did not reach Completed (events = {:?})",
        events.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>()
    );

    Spies { pre, post, stop }
}

// ---------------------------------------------------------------------------
// Tests — one per dispatch site, each asserts the ADR-V2-006 literal
// ---------------------------------------------------------------------------

/// harness.rs:2236 — PreToolUse dispatch must reach the spy with
/// `ctx.event = Some("PreToolUse")`.
#[tokio::test]
async fn test_pre_tool_use_dispatch_sets_event_literal() {
    let spies = run_with_spies().await;
    let pre = spies.pre.lock().unwrap().clone();
    assert!(
        !pre.is_empty(),
        "PreToolUse spy never fired — dispatch site at harness.rs:2236 did not run"
    );
    assert!(
        pre.iter().all(|e| e.as_deref() == Some("PreToolUse")),
        "PreToolUse dispatch site must set ctx.event = \"PreToolUse\" \
         (ADR-V2-006 §2.1); captured = {:?}",
        pre
    );
}

/// harness.rs:2390 — PostToolUse dispatch must reach the spy with
/// `ctx.event = Some("PostToolUse")`.
#[tokio::test]
async fn test_post_tool_use_dispatch_sets_event_literal() {
    let spies = run_with_spies().await;
    let post = spies.post.lock().unwrap().clone();
    assert!(
        !post.is_empty(),
        "PostToolUse spy never fired — dispatch site at harness.rs:2390 did not run"
    );
    assert!(
        post.iter().all(|e| e.as_deref() == Some("PostToolUse")),
        "PostToolUse dispatch site must set ctx.event = \"PostToolUse\" \
         (ADR-V2-006 §2.2); captured = {:?}",
        post
    );
}

/// harness.rs:1766 — Stop dispatch (inside `dispatch_stop_hooks`) must reach
/// the spy with `ctx.event = Some("Stop")`.
#[tokio::test]
async fn test_stop_dispatch_sets_event_literal() {
    let spies = run_with_spies().await;
    let stop = spies.stop.lock().unwrap().clone();
    assert!(
        !stop.is_empty(),
        "Stop spy never fired — dispatch site at harness.rs:1766 did not run"
    );
    assert!(
        stop.iter().all(|e| e.as_deref() == Some("Stop")),
        "Stop dispatch site must set ctx.event = \"Stop\" \
         (ADR-V2-006 §2.3); captured = {:?}",
        stop
    );
}
