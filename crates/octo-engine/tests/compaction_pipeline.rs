//! Integration tests for CompactionPipeline (AP-T6).

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlock, SandboxId, StopReason,
    TokenUsage, UserId,
};

use octo_engine::context::{CompactionContext, CompactionPipeline, CompactionPipelineConfig};
use octo_engine::providers::{CompletionStream, Provider};

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

struct MockSummaryProvider {
    /// The text to return from `complete()`.
    response_text: String,
}

impl MockSummaryProvider {
    fn new(text: impl Into<String>) -> Self {
        Self {
            response_text: text.into(),
        }
    }
}

#[async_trait]
impl Provider for MockSummaryProvider {
    fn id(&self) -> &str {
        "mock-summary"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse {
            id: "mock-resp".into(),
            content: vec![ContentBlock::Text {
                text: self.response_text.clone(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        })
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("stream not supported in mock"))
    }
}

/// Mock provider that always returns PTL error.
struct MockPtlProvider;

#[async_trait]
impl Provider for MockPtlProvider {
    fn id(&self) -> &str {
        "mock-ptl"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        Err(anyhow::anyhow!("prompt_too_long: input exceeds maximum context length"))
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("prompt_too_long"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_messages(count: usize) -> Vec<ChatMessage> {
    let mut msgs = Vec::new();
    for i in 0..count {
        if i % 2 == 0 {
            msgs.push(ChatMessage::user(format!("User message {}", i)));
        } else {
            msgs.push(ChatMessage::assistant(format!("Assistant response {}", i)));
        }
    }
    msgs
}

fn default_context() -> CompactionContext {
    CompactionContext {
        memory: None,
        memory_store: None,
        active_skill: None,
        hook_registry: None,
        session_summary_store: None,
        user_id: UserId::from_string("test-user"),
        sandbox_id: SandboxId::from_string("test-sandbox"),
        custom_instructions: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compact_basic_flow() {
    let provider = MockSummaryProvider::new(
        "<analysis>Analysis here</analysis>\n<summary>\n1. **Primary Requests**: User asked about Rust\n</summary>"
    );
    let pipeline = CompactionPipeline::new(CompactionPipelineConfig {
        keep_recent_messages: 2,
        ..Default::default()
    });

    let messages = make_messages(10);
    let ctx = default_context();

    let result = pipeline
        .compact(&messages, &provider, "test-model", &ctx)
        .await
        .expect("compact should succeed");

    // boundary marker + summary + kept messages
    assert_eq!(result.kept_messages.len(), 2);
    assert_eq!(result.summary_messages.len(), 1);

    // Summary should have analysis stripped
    let summary_text = result.summary_messages[0].text_content();
    assert!(!summary_text.contains("<analysis>"));
    assert!(summary_text.contains("Primary Requests"));
    assert!(summary_text.contains("continued from a previous conversation"));

    // Token estimates should be populated
    assert!(result.pre_compact_tokens > 0);
    assert!(result.post_compact_tokens > 0);
}

#[tokio::test]
async fn test_compact_too_few_messages() {
    let provider = MockSummaryProvider::new("summary");
    let pipeline = CompactionPipeline::new(CompactionPipelineConfig {
        keep_recent_messages: 6,
        ..Default::default()
    });

    // Only 3 messages — boundary would be < 2
    let messages = make_messages(3);
    let ctx = default_context();

    let result = pipeline
        .compact(&messages, &provider, "test-model", &ctx)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Not enough messages"));
}

#[tokio::test]
async fn test_compact_ptl_all_retries_fail() {
    let provider = MockPtlProvider;
    let pipeline = CompactionPipeline::new(CompactionPipelineConfig {
        keep_recent_messages: 2,
        max_ptl_retries: 2,
        ..Default::default()
    });

    let messages = make_messages(20);
    let ctx = default_context();

    let result = pipeline
        .compact(&messages, &provider, "test-model", &ctx)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("PTL retries"));
}

#[tokio::test]
async fn test_compact_with_custom_instructions() {
    let provider = MockSummaryProvider::new("<summary>\nCustom summary\n</summary>");
    let pipeline = CompactionPipeline::new(CompactionPipelineConfig {
        keep_recent_messages: 2,
        ..Default::default()
    });

    let messages = make_messages(10);
    let ctx = CompactionContext {
        custom_instructions: Some("Always preserve Rust code snippets".into()),
        ..default_context()
    };

    let result = pipeline
        .compact(&messages, &provider, "test-model", &ctx)
        .await
        .expect("compact should succeed");

    assert!(result.summary_messages[0]
        .text_content()
        .contains("Custom summary"));
}

#[tokio::test]
async fn test_compact_message_reassembly() {
    let provider = MockSummaryProvider::new("<summary>\nSummary text\n</summary>");
    let pipeline = CompactionPipeline::new(CompactionPipelineConfig {
        keep_recent_messages: 3,
        ..Default::default()
    });

    let messages = make_messages(12);
    let ctx = default_context();

    let result = pipeline
        .compact(&messages, &provider, "test-model", &ctx)
        .await
        .expect("compact should succeed");

    // Verify the message ordering: boundary + summary + kept + reinjections
    assert_eq!(
        result.boundary_marker.text_content(),
        "[Context compacted: earlier conversation summarized below]"
    );
    assert_eq!(result.kept_messages.len(), 3);
    // Last 3 messages should be the original last 3
    assert_eq!(
        result.kept_messages[0].text_content(),
        messages[9].text_content()
    );
    assert_eq!(
        result.kept_messages[2].text_content(),
        messages[11].text_content()
    );
}

#[test]
fn test_format_summary_nested_tags() {
    let raw = "<analysis>
Deep analysis of conversation:
- User wants X
- Assistant did Y
</analysis>

<summary>
1. **Primary Requests**: Build a REST API
2. **Key Technical Concepts**: Axum, Tokio, SQLite
</summary>";
    let result = CompactionPipeline::format_summary(raw);
    assert!(!result.contains("<analysis>"));
    assert!(result.contains("Build a REST API"));
    assert!(result.contains("Axum, Tokio, SQLite"));
}

#[test]
fn test_format_summary_no_summary_tag() {
    let raw = "Here is the conversation summary without tags:\n1. User asked about foo\n2. We fixed bar";
    let result = CompactionPipeline::format_summary(raw);
    assert!(result.contains("User asked about foo"));
    assert!(result.contains("continued from a previous conversation"));
}

#[test]
fn test_compaction_config_defaults() {
    let config = CompactionPipelineConfig::default();
    assert_eq!(config.summary_max_tokens, 2000);
    assert_eq!(config.keep_recent_messages, 6);
    assert_eq!(config.max_ptl_retries, 3);
    assert!(config.compact_model.is_none());
}
