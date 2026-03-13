//! Mock and Replay providers for zero-cost evaluation runs.
//!
//! - [`MockProvider`] returns pre-configured responses in sequence.
//! - [`ReplayProvider`] loads recorded LLM interactions from JSONL files.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use octo_engine::providers::{CompletionStream, Provider};
use octo_types::{
    CompletionRequest, CompletionResponse, ContentBlock, StopReason, TokenUsage,
};

// ============================================================
// MockProvider — returns pre-configured responses in sequence
// ============================================================

/// A provider that returns pre-configured [`CompletionResponse`] values in
/// order.  When responses are exhausted it returns a sentinel end-turn message.
///
/// Every call to [`Provider::complete`] is recorded so tests can assert on the
/// requests that were sent.
pub struct MockProvider {
    responses: Vec<CompletionResponse>,
    cursor: AtomicUsize,
    call_log: Mutex<Vec<CompletionRequest>>,
}

impl MockProvider {
    /// Create a new mock provider with the given sequence of responses.
    pub fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses,
            cursor: AtomicUsize::new(0),
            call_log: Mutex::new(Vec::new()),
        }
    }

    /// Convenience: create a provider that always returns a single text response.
    pub fn with_text(text: &str) -> Self {
        Self::new(vec![CompletionResponse {
            id: "mock-1".into(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        }])
    }

    /// Convenience: create a provider that returns a single tool-use response.
    pub fn with_tool_call(tool_name: &str, tool_input: serde_json::Value) -> Self {
        Self::new(vec![CompletionResponse {
            id: "mock-1".into(),
            content: vec![ContentBlock::ToolUse {
                id: "call_mock_1".into(),
                name: tool_name.to_string(),
                input: tool_input,
            }],
            stop_reason: Some(StopReason::ToolUse),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 15,
            },
        }])
    }

    /// Convenience: create a provider that first returns a tool call, then a
    /// text response (simulating a tool-use agent loop).
    pub fn with_tool_then_text(
        tool_name: &str,
        tool_input: serde_json::Value,
        text: &str,
    ) -> Self {
        Self::new(vec![
            CompletionResponse {
                id: "mock-1".into(),
                content: vec![ContentBlock::ToolUse {
                    id: "call_mock_1".into(),
                    name: tool_name.to_string(),
                    input: tool_input,
                }],
                stop_reason: Some(StopReason::ToolUse),
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 15,
                },
            },
            CompletionResponse {
                id: "mock-2".into(),
                content: vec![ContentBlock::Text {
                    text: text.to_string(),
                }],
                stop_reason: Some(StopReason::EndTurn),
                usage: TokenUsage {
                    input_tokens: 20,
                    output_tokens: 10,
                },
            },
        ])
    }

    /// How many times [`Provider::complete`] has been called.
    pub fn call_count(&self) -> usize {
        self.cursor.load(Ordering::SeqCst)
    }

    /// Clone of every [`CompletionRequest`] received so far.
    pub fn calls(&self) -> Vec<CompletionRequest> {
        self.call_log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.call_log.lock().unwrap().push(request);
        let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
        if idx < self.responses.len() {
            Ok(self.responses[idx].clone())
        } else {
            Ok(CompletionResponse {
                id: format!("mock-overflow-{idx}"),
                content: vec![ContentBlock::Text {
                    text: "[MockProvider: no more responses configured]".into(),
                }],
                stop_reason: Some(StopReason::EndTurn),
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            })
        }
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        anyhow::bail!("MockProvider does not support streaming")
    }
}

// ============================================================
// ReplayProvider — replays recorded interactions from JSONL
// ============================================================

/// A single recorded LLM interaction (request summary + full response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedInteraction {
    /// Human-readable summary of the request (for debugging).
    pub request_summary: String,
    /// The full response in a serializable form.
    pub response: SerializableResponse,
    /// Original latency in milliseconds (informational only).
    pub latency_ms: u64,
}

/// Serializable mirror of [`CompletionResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableResponse {
    pub id: String,
    pub content: Vec<SerializableContent>,
    pub stop_reason: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Serializable mirror of [`ContentBlock`] (text and tool_use only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SerializableContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

impl SerializableResponse {
    /// Convert back to the engine's [`CompletionResponse`].
    pub fn to_completion_response(&self) -> CompletionResponse {
        let content = self
            .content
            .iter()
            .map(|c| match c {
                SerializableContent::Text { text } => ContentBlock::Text { text: text.clone() },
                SerializableContent::ToolUse { id, name, input } => ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
            })
            .collect();

        let stop_reason = self.stop_reason.as_deref().map(|s| match s {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            "stop_sequence" => StopReason::StopSequence,
            _ => StopReason::EndTurn,
        });

        CompletionResponse {
            id: self.id.clone(),
            content,
            stop_reason,
            usage: TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
            },
        }
    }
}

/// A provider that replays [`RecordedInteraction`]s loaded from a JSONL file.
///
/// Each call to [`Provider::complete`] returns the next recorded response in
/// order.  When interactions are exhausted it returns a sentinel end-turn
/// message.
pub struct ReplayProvider {
    interactions: Vec<RecordedInteraction>,
    cursor: AtomicUsize,
}

impl ReplayProvider {
    /// Create from an already-parsed vector of interactions.
    pub fn new(interactions: Vec<RecordedInteraction>) -> Self {
        Self {
            interactions,
            cursor: AtomicUsize::new(0),
        }
    }

    /// Load interactions from a JSONL file (one JSON object per line).
    pub fn from_jsonl(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let interactions: Vec<RecordedInteraction> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self::new(interactions))
    }

    /// Write interactions to a JSONL file.
    pub fn save_jsonl(interactions: &[RecordedInteraction], path: &Path) -> Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        for interaction in interactions {
            let line = serde_json::to_string(interaction)?;
            writeln!(f, "{line}")?;
        }
        Ok(())
    }

    /// How many interactions are loaded.
    pub fn len(&self) -> usize {
        self.interactions.len()
    }

    /// Whether the interaction list is empty.
    pub fn is_empty(&self) -> bool {
        self.interactions.is_empty()
    }
}

#[async_trait]
impl Provider for ReplayProvider {
    fn id(&self) -> &str {
        "replay"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
        if idx < self.interactions.len() {
            Ok(self.interactions[idx].response.to_completion_response())
        } else {
            Ok(CompletionResponse {
                id: format!("replay-overflow-{idx}"),
                content: vec![ContentBlock::Text {
                    text: "[ReplayProvider: no more recorded interactions]".into(),
                }],
                stop_reason: Some(StopReason::EndTurn),
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            })
        }
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        anyhow::bail!("ReplayProvider does not support streaming")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_returns_configured_responses() {
        let provider = MockProvider::with_text("hello");
        let req = CompletionRequest {
            model: "test".into(),
            system: None,
            messages: vec![],
            max_tokens: 100,
            temperature: None,
            tools: vec![],
            stream: false,
        };

        let resp = provider.complete(req).await.unwrap();
        assert_eq!(resp.id, "mock-1");
        assert_eq!(provider.call_count(), 1);

        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mock_provider_overflow_returns_sentinel() {
        let provider = MockProvider::with_text("only-one");
        let req = || CompletionRequest {
            model: "test".into(),
            system: None,
            messages: vec![],
            max_tokens: 100,
            temperature: None,
            tools: vec![],
            stream: false,
        };

        let _ = provider.complete(req()).await.unwrap();
        let resp = provider.complete(req()).await.unwrap();
        assert!(resp.id.starts_with("mock-overflow-"));
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn mock_provider_tool_then_text() {
        let provider = MockProvider::with_tool_then_text(
            "bash",
            serde_json::json!({"command": "ls"}),
            "done",
        );
        let req = || CompletionRequest {
            model: "test".into(),
            system: None,
            messages: vec![],
            max_tokens: 100,
            temperature: None,
            tools: vec![],
            stream: false,
        };

        let r1 = provider.complete(req()).await.unwrap();
        assert!(matches!(r1.stop_reason, Some(StopReason::ToolUse)));

        let r2 = provider.complete(req()).await.unwrap();
        assert!(matches!(r2.stop_reason, Some(StopReason::EndTurn)));
        match &r2.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "done"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replay_provider_roundtrip_jsonl() {
        let interactions = vec![
            RecordedInteraction {
                request_summary: "user says hi".into(),
                response: SerializableResponse {
                    id: "resp-1".into(),
                    content: vec![SerializableContent::Text {
                        text: "Hello!".into(),
                    }],
                    stop_reason: Some("end_turn".into()),
                    input_tokens: 5,
                    output_tokens: 3,
                },
                latency_ms: 200,
            },
            RecordedInteraction {
                request_summary: "user asks for tool".into(),
                response: SerializableResponse {
                    id: "resp-2".into(),
                    content: vec![SerializableContent::ToolUse {
                        id: "call_1".into(),
                        name: "bash".into(),
                        input: serde_json::json!({"cmd": "echo hi"}),
                    }],
                    stop_reason: Some("tool_use".into()),
                    input_tokens: 10,
                    output_tokens: 8,
                },
                latency_ms: 350,
            },
        ];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interactions.jsonl");

        ReplayProvider::save_jsonl(&interactions, &path).unwrap();
        let provider = ReplayProvider::from_jsonl(&path).unwrap();
        assert_eq!(provider.len(), 2);

        let req = || CompletionRequest {
            model: "test".into(),
            system: None,
            messages: vec![],
            max_tokens: 100,
            temperature: None,
            tools: vec![],
            stream: false,
        };

        let r1 = provider.complete(req()).await.unwrap();
        assert_eq!(r1.id, "resp-1");
        match &r1.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello!"),
            other => panic!("expected Text, got {other:?}"),
        }

        let r2 = provider.complete(req()).await.unwrap();
        assert_eq!(r2.id, "resp-2");
        assert!(matches!(r2.stop_reason, Some(StopReason::ToolUse)));

        // Overflow
        let r3 = provider.complete(req()).await.unwrap();
        assert!(r3.id.starts_with("replay-overflow-"));
    }

    #[tokio::test]
    async fn serializable_response_stop_reason_mapping() {
        let cases = vec![
            ("end_turn", StopReason::EndTurn),
            ("tool_use", StopReason::ToolUse),
            ("max_tokens", StopReason::MaxTokens),
            ("stop_sequence", StopReason::StopSequence),
            ("unknown_value", StopReason::EndTurn), // fallback
        ];

        for (input, expected) in cases {
            let sr = SerializableResponse {
                id: "test".into(),
                content: vec![],
                stop_reason: Some(input.into()),
                input_tokens: 0,
                output_tokens: 0,
            };
            let resp = sr.to_completion_response();
            assert_eq!(
                resp.stop_reason.unwrap(),
                expected,
                "stop_reason mapping failed for '{input}'"
            );
        }
    }
}
