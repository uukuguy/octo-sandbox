//! Assessment tests for provider failover chain.
//!
//! Three focused tests validating retry behaviour through the pipeline builder:
//! 1. Retry then succeed — preserves response content
//! 2. All retries exhausted — returns error
//! 3. Zero failures — immediate pass-through

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::message::ContentBlock;
use octo_types::provider::TokenUsage;
use octo_types::{CompletionRequest, CompletionResponse};

use octo_engine::providers::pipeline::RetryProvider;
use octo_engine::providers::traits::{CompletionStream, Provider};
use octo_engine::providers::RetryPolicy;

// ---------------------------------------------------------------------------
// Mock Providers
// ---------------------------------------------------------------------------

/// Always returns an error on `complete()`.
struct AlwaysFailProvider {
    call_count: AtomicU32,
}

impl AlwaysFailProvider {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for AlwaysFailProvider {
    fn id(&self) -> &str {
        "always-fail"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Err(anyhow::anyhow!("HTTP 500 simulated server error"))
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("stream not implemented"))
    }
}

/// Fails `fail_until` times, then succeeds on the next call.
struct SucceedOnNthProvider {
    fail_until: u32,
    call_count: AtomicU32,
}

impl SucceedOnNthProvider {
    fn new(fail_until: u32) -> Self {
        Self {
            fail_until,
            call_count: AtomicU32::new(0),
        }
    }

    fn calls(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for SucceedOnNthProvider {
    fn id(&self) -> &str {
        "succeed-on-nth"
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        if call < self.fail_until {
            return Err(anyhow::anyhow!("HTTP 503 service unavailable"));
        }
        Ok(CompletionResponse {
            id: format!("resp-{}", call),
            content: vec![ContentBlock::Text {
                text: "failover success".into(),
            }],
            stop_reason: None,
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        })
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("stream not implemented"))
    }
}

// ---------------------------------------------------------------------------
// Helper: ArcProvider — thin wrapper to share mock via Arc for call inspection
// ---------------------------------------------------------------------------

struct ArcProvider<T: Provider + Send + Sync>(Arc<T>);

#[async_trait]
impl<T: Provider + Send + Sync> Provider for ArcProvider<T> {
    fn id(&self) -> &str {
        self.0.id()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.0.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        self.0.stream(request).await
    }
}

// ---------------------------------------------------------------------------
// 1. Retry then succeed — preserves response content
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_then_succeed_preserves_response() {
    // Fail 2 times, succeed on the 3rd call.
    let mock = Arc::new(SucceedOnNthProvider::new(2));

    let policy = RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(Box::new(ArcProvider(Arc::clone(&mock))), policy);

    let resp = provider.complete(CompletionRequest::default()).await;
    assert!(resp.is_ok(), "Should succeed after retries: {:?}", resp.err());

    let resp = resp.unwrap();

    // Verify response content is preserved.
    assert_eq!(resp.content.len(), 1);
    match &resp.content[0] {
        ContentBlock::Text { text } => assert_eq!(text, "failover success"),
        other => panic!("Expected Text content block, got: {:?}", other),
    }

    // Verify token usage is preserved.
    assert_eq!(resp.usage.input_tokens, 100);
    assert_eq!(resp.usage.output_tokens, 50);

    // 2 failures + 1 success = 3 total calls.
    assert_eq!(mock.calls(), 3);
}

// ---------------------------------------------------------------------------
// 2. All retries exhausted — returns error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_retries_exhausted_returns_error() {
    let mock = Arc::new(AlwaysFailProvider::new());

    let policy = RetryPolicy {
        max_retries: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(Box::new(ArcProvider(Arc::clone(&mock))), policy);

    let resp = provider.complete(CompletionRequest::default()).await;
    assert!(resp.is_err(), "Should fail after exhausting all retries");

    let err_msg = resp.unwrap_err().to_string();
    assert!(
        err_msg.contains("500"),
        "Error should contain the original error message, got: {}",
        err_msg
    );

    // 1 initial + 2 retries = 3 total calls.
    assert_eq!(mock.calls(), 3);
}

// ---------------------------------------------------------------------------
// 3. Zero failures — immediate pass-through
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zero_failures_passes_through() {
    // fail_until=0 means always succeed.
    let mock = Arc::new(SucceedOnNthProvider::new(0));

    let policy = RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(Box::new(ArcProvider(Arc::clone(&mock))), policy);

    let resp = provider.complete(CompletionRequest::default()).await;
    assert!(resp.is_ok(), "Should succeed immediately: {:?}", resp.err());

    let resp = resp.unwrap();

    // Verify response content.
    assert_eq!(resp.content.len(), 1);
    match &resp.content[0] {
        ContentBlock::Text { text } => assert_eq!(text, "failover success"),
        other => panic!("Expected Text content block, got: {:?}", other),
    }

    // Only 1 call — no retries needed.
    assert_eq!(mock.calls(), 1);
}
