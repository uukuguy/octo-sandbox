//! Harness integration tests — mock provider + mock tool full flow.
//!
//! Tests the complete agent loop: user message -> LLM response -> tool call -> tool result -> final answer.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde_json::json;

use octo_engine::agent::{run_agent_loop, AgentConfig, AgentEvent, AgentLoopConfig, CancellationToken};
use octo_engine::providers::{CompletionStream, Provider};
use octo_engine::tools::{Tool, ToolRegistry};
use octo_types::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlock, StopReason, StreamEvent,
    ToolContext, ToolOutput, ToolSource, TokenUsage,
};

// ---------------------------------------------------------------------------
// MockProvider — configurable multi-call provider
// ---------------------------------------------------------------------------

/// A mock provider that returns different responses on successive calls.
/// On the first call it returns a tool_use response; on the second call
/// it returns a text-only final answer.
struct MockProvider {
    call_count: AtomicU32,
    /// If true, the first call returns a tool_use; otherwise text-only.
    use_tool: bool,
}

impl MockProvider {
    fn new(use_tool: bool) -> Self {
        Self {
            call_count: AtomicU32::new(0),
            use_tool,
        }
    }

    /// Build a stream of events for a text-only response.
    fn text_stream(text: &str) -> CompletionStream {
        let events: Vec<Result<StreamEvent>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg_test_001".into(),
            }),
            Ok(StreamEvent::TextDelta {
                text: text.to_string(),
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 50,
                },
            }),
        ];
        Box::pin(stream::iter(events))
    }

    /// Build a stream of events for a tool_use response.
    fn tool_use_stream() -> CompletionStream {
        let tool_input = json!({"message": "hello from test"});
        let events: Vec<Result<StreamEvent>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg_test_002".into(),
            }),
            Ok(StreamEvent::TextDelta {
                text: "Let me call the tool.".into(),
            }),
            Ok(StreamEvent::ToolUseComplete {
                index: 0,
                id: "toolu_test_001".into(),
                name: "mock_echo".into(),
                input: tool_input,
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage {
                    input_tokens: 120,
                    output_tokens: 60,
                },
            }),
        ];
        Box::pin(stream::iter(events))
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse {
            id: "resp_mock".into(),
            content: vec![ContentBlock::Text {
                text: "mock response".into(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        })
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.use_tool && n == 0 {
            // First call: return tool_use
            Ok(Self::tool_use_stream())
        } else {
            // Subsequent calls (or text-only mode): return text
            Ok(Self::text_stream("The answer is 42."))
        }
    }
}

// ---------------------------------------------------------------------------
// MockEchoTool — simple tool that echoes input
// ---------------------------------------------------------------------------

struct MockEchoTool;

#[async_trait]
impl Tool for MockEchoTool {
    fn name(&self) -> &str {
        "mock_echo"
    }

    fn description(&self) -> &str {
        "Echoes the input message back"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Message to echo" }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let msg = params
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(no message)");
        Ok(ToolOutput::success(format!("Echo: {msg}")))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

// ---------------------------------------------------------------------------
// Helper: build a minimal AgentLoopConfig
// ---------------------------------------------------------------------------

fn build_config(provider: Arc<dyn Provider>, registry: Arc<ToolRegistry>) -> AgentLoopConfig {
    AgentLoopConfig::builder()
        .provider(provider)
        .tools(registry)
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(10)
        .force_text_at_last(true)
        .agent_config(AgentConfig {
            enable_typing_signal: false,
            enable_parallel: false,
            ..AgentConfig::default()
        })
        .build()
}

fn make_user_message(text: &str) -> Vec<ChatMessage> {
    vec![ChatMessage::user(text)]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full tool-call flow: user msg -> LLM returns tool_use -> tool executes -> LLM returns text.
#[tokio::test]
async fn test_harness_full_tool_call_flow() {
    let provider = Arc::new(MockProvider::new(true));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let config = build_config(provider, registry);
    let messages = make_user_message("Please call the echo tool.");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // Verify we got the expected event sequence.
    // Round 0: IterationStart -> TextDelta -> ToolStart -> ToolResult -> IterationEnd
    // Round 1: IterationStart -> TextDelta -> TextComplete -> IterationEnd -> Completed -> Done

    let mut saw_iteration_start_0 = false;
    let mut saw_tool_start = false;
    let mut saw_tool_result = false;
    let mut saw_text_delta = false;
    let mut saw_text_complete = false;
    let mut saw_completed = false;
    let mut saw_done = false;
    let mut tool_result_output = String::new();
    let mut completed_rounds = 0u32;
    let mut completed_tool_calls = 0u32;

    for event in &events {
        match event {
            AgentEvent::IterationStart { round: 0 } => saw_iteration_start_0 = true,
            AgentEvent::ToolStart {
                tool_name, ..
            } => {
                assert_eq!(tool_name, "mock_echo");
                saw_tool_start = true;
            }
            AgentEvent::ToolResult {
                output, success, ..
            } => {
                assert!(success);
                assert!(output.contains("Echo:"));
                tool_result_output = output.clone();
                saw_tool_result = true;
            }
            AgentEvent::TextDelta { .. } => saw_text_delta = true,
            AgentEvent::TextComplete { text } => {
                assert_eq!(text, "The answer is 42.");
                saw_text_complete = true;
            }
            AgentEvent::Completed(result) => {
                completed_rounds = result.rounds;
                completed_tool_calls = result.tool_calls;
                saw_completed = true;
            }
            AgentEvent::Done => saw_done = true,
            _ => {}
        }
    }

    assert!(saw_iteration_start_0, "Expected IterationStart for round 0");
    assert!(saw_tool_start, "Expected ToolStart event");
    assert!(saw_tool_result, "Expected ToolResult event");
    assert!(
        tool_result_output.contains("Echo: hello from test"),
        "Tool result should contain echoed message, got: {}",
        tool_result_output
    );
    assert!(saw_text_delta, "Expected TextDelta event");
    assert!(saw_text_complete, "Expected TextComplete event");
    assert!(saw_completed, "Expected Completed event");
    assert!(saw_done, "Expected Done event");
    assert_eq!(completed_rounds, 2, "Should complete in 2 rounds (tool call + final answer)");
    assert_eq!(completed_tool_calls, 1, "Should have exactly 1 tool call");
}

/// Text-only response — no tool calls, just a direct answer.
#[tokio::test]
async fn test_harness_text_only_response() {
    let provider = Arc::new(MockProvider::new(false));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let config = build_config(provider, registry);
    let messages = make_user_message("What is the meaning of life?");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    let mut saw_iteration_start = false;
    let mut saw_text_complete = false;
    let mut saw_completed = false;
    let mut saw_done = false;
    let mut saw_tool_start = false;

    for event in &events {
        match event {
            AgentEvent::IterationStart { round: 0 } => saw_iteration_start = true,
            AgentEvent::TextComplete { text } => {
                assert_eq!(text, "The answer is 42.");
                saw_text_complete = true;
            }
            AgentEvent::Completed(result) => {
                assert_eq!(result.rounds, 1, "Text-only should complete in 1 round");
                assert_eq!(result.tool_calls, 0, "No tool calls expected");
                saw_completed = true;
            }
            AgentEvent::Done => saw_done = true,
            AgentEvent::ToolStart { .. } => saw_tool_start = true,
            _ => {}
        }
    }

    assert!(saw_iteration_start, "Expected IterationStart");
    assert!(saw_text_complete, "Expected TextComplete");
    assert!(saw_completed, "Expected Completed");
    assert!(saw_done, "Expected Done");
    assert!(!saw_tool_start, "Should NOT have any ToolStart events");
}

/// Cancellation: cancel the token before the loop starts and verify it stops quickly.
#[tokio::test]
async fn test_harness_cancellation() {
    let provider = Arc::new(MockProvider::new(true));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let cancel_token = CancellationToken::new();
    // Cancel immediately — the harness should detect this at the top of the first round.
    cancel_token.cancel();

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(registry)
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(10)
        .cancel_token(cancel_token)
        .agent_config(AgentConfig {
            enable_typing_signal: false,
            enable_parallel: false,
            ..AgentConfig::default()
        })
        .build();

    let messages = make_user_message("This should be cancelled.");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // When cancelled at the top of round 0, the harness emits IterationStart(0) then
    // breaks out of the loop, hitting the max-rounds-exceeded path which emits
    // Error + Completed + Done. The key point is that no tool calls happen.
    let mut saw_tool_start = false;
    let mut saw_text_complete = false;

    for event in &events {
        match event {
            AgentEvent::ToolStart { .. } => saw_tool_start = true,
            AgentEvent::TextComplete { .. } => saw_text_complete = true,
            _ => {}
        }
    }

    assert!(!saw_tool_start, "Should NOT execute any tools when cancelled");
    // The loop should have ended early — no full text completion from LLM
    assert!(!saw_text_complete, "Should NOT have TextComplete when cancelled before LLM call");
}

/// Max iterations: set max_iterations=1 with a tool-calling provider.
/// The harness should stop after 1 round (the tool call round), hitting the max-rounds limit.
#[tokio::test]
async fn test_harness_max_iterations() {
    let provider = Arc::new(MockProvider::new(true));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let config = AgentLoopConfig::builder()
        .provider(provider)
        .tools(registry)
        .model("mock-model".into())
        .max_tokens(1024)
        .max_iterations(1)
        .force_text_at_last(true)
        .agent_config(AgentConfig {
            enable_typing_signal: false,
            enable_parallel: false,
            ..AgentConfig::default()
        })
        .build();

    let messages = make_user_message("Call the tool please.");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // With max_iterations=1 and force_text_at_last=true, the single round
    // should force text-only (no tools). The provider returns text, so we get
    // a normal text completion in 1 round.
    let mut saw_completed = false;
    let mut completed_rounds = 0u32;

    for event in &events {
        if let AgentEvent::Completed(result) = event {
            completed_rounds = result.rounds;
            saw_completed = true;
        }
    }

    assert!(saw_completed, "Expected Completed event");
    assert_eq!(
        completed_rounds, 1,
        "With max_iterations=1, should complete in exactly 1 round"
    );
}

/// Verify event ordering: IterationStart always before IterationEnd for same round.
#[tokio::test]
async fn test_harness_event_ordering() {
    let provider = Arc::new(MockProvider::new(true));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let config = build_config(provider, registry);
    let messages = make_user_message("Test event ordering.");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    // Track iteration start/end ordering
    let mut open_rounds: Vec<u32> = Vec::new();
    let mut last_done_idx = None;
    let mut last_completed_idx = None;

    for (i, event) in events.iter().enumerate() {
        match event {
            AgentEvent::IterationStart { round } => {
                open_rounds.push(*round);
            }
            AgentEvent::IterationEnd { round } => {
                assert!(
                    open_rounds.last() == Some(round),
                    "IterationEnd({round}) without matching IterationStart"
                );
                open_rounds.pop();
            }
            AgentEvent::Completed(_) => last_completed_idx = Some(i),
            AgentEvent::Done => last_done_idx = Some(i),
            _ => {}
        }
    }

    assert!(
        open_rounds.is_empty(),
        "All IterationStart events should have matching IterationEnd"
    );
    assert!(last_completed_idx.is_some(), "Must have Completed event");
    assert!(last_done_idx.is_some(), "Must have Done event");
    assert!(
        last_completed_idx.unwrap() < last_done_idx.unwrap(),
        "Completed must come before Done"
    );
}

/// No tools configured: harness should return error + done when ToolRegistry is None.
/// (This tests the guard at the top of run_agent_loop_inner.)
#[tokio::test]
async fn test_harness_no_tools_returns_error() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider::new(false));

    let config = AgentLoopConfig::builder()
        .provider(provider)
        // No .tools() call — tools remains None
        .model("mock-model".into())
        .build();

    let events: Vec<AgentEvent> = run_agent_loop(config, vec![]).collect().await;

    assert!(events.len() >= 2);
    assert!(
        matches!(&events[0], AgentEvent::Error { message } if message.contains("tool registry")),
        "First event should be Error about missing tool registry"
    );
    assert!(matches!(events.last().unwrap(), AgentEvent::Done));
}

/// Verify that final_messages in Completed event contains the conversation history.
#[tokio::test]
async fn test_harness_final_messages_populated() {
    let provider = Arc::new(MockProvider::new(true));
    let mut registry = ToolRegistry::new();
    registry.register(MockEchoTool);
    let registry = Arc::new(registry);

    let config = build_config(provider, registry);
    let messages = make_user_message("Check final messages.");

    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;

    let completed = events.iter().find_map(|e| {
        if let AgentEvent::Completed(result) = e {
            Some(result)
        } else {
            None
        }
    });

    let result = completed.expect("Must have Completed event");
    assert!(
        !result.final_messages.is_empty(),
        "final_messages should not be empty"
    );
    // Should have at least: original user msg, assistant (tool call), user (tool result),
    // assistant (final answer)
    assert!(
        result.final_messages.len() >= 4,
        "Expected at least 4 messages in final_messages (user, assistant+tool, tool_result, final_answer), got {}",
        result.final_messages.len()
    );
}
