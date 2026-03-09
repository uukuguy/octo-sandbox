//! Tests for the provider pipeline decorator chain.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::message::ContentBlock;
use octo_types::provider::TokenUsage;
use octo_types::{CompletionRequest, CompletionResponse};

use octo_engine::providers::pipeline::{
    CircuitBreakerConfig, CircuitBreakerProvider, CircuitState, CostBudget, CostGuardProvider,
    ProviderPipelineBuilder, RetryProvider,
};
use octo_engine::providers::traits::{CompletionStream, Provider};
use octo_engine::providers::RetryPolicy;

// ---------------------------------------------------------------------------
// Mock Provider
// ---------------------------------------------------------------------------

/// A configurable mock provider for testing.
struct MockProvider {
    id: String,
    /// Number of calls before succeeding (0 = always succeed).
    fail_count: AtomicU32,
    /// The error message to return on failure.
    error_msg: String,
    /// Total number of calls made.
    call_count: AtomicU32,
    /// Token usage to return on success.
    usage: TokenUsage,
}

impl MockProvider {
    fn always_ok() -> Self {
        Self {
            id: "mock".to_string(),
            fail_count: AtomicU32::new(0),
            error_msg: String::new(),
            call_count: AtomicU32::new(0),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        }
    }

    fn always_fail(error_msg: &str) -> Self {
        Self {
            id: "mock".to_string(),
            fail_count: AtomicU32::new(u32::MAX),
            error_msg: error_msg.to_string(),
            call_count: AtomicU32::new(0),
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        }
    }

    /// Fail `n` times, then succeed.
    fn fail_then_ok(n: u32, error_msg: &str) -> Self {
        Self {
            id: "mock".to_string(),
            fail_count: AtomicU32::new(n),
            error_msg: error_msg.to_string(),
            call_count: AtomicU32::new(0),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        }
    }

    fn with_usage(mut self, input: u32, output: u32) -> Self {
        self.usage = TokenUsage {
            input_tokens: input,
            output_tokens: output,
        };
        self
    }

    fn calls(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        let remaining = self.fail_count.load(Ordering::SeqCst);
        if remaining > 0 && call < remaining {
            return Err(anyhow::anyhow!("{}", self.error_msg));
        }
        Ok(CompletionResponse {
            id: format!("resp-{}", call),
            content: vec![ContentBlock::Text { text: "ok".into() }],
            stop_reason: None,
            usage: self.usage.clone(),
        })
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("stream not implemented in mock"))
    }
}

fn default_request() -> CompletionRequest {
    CompletionRequest::default()
}

// ---------------------------------------------------------------------------
// 1. Pipeline Builder Basic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_builder_basic() {
    let provider = ProviderPipelineBuilder::new(Box::new(MockProvider::always_ok()))
        .with_retry(RetryPolicy::default())
        .with_circuit_breaker(CircuitBreakerConfig::default())
        .with_cost_guard(CostBudget::daily(10.0))
        .build();

    // The pipeline should delegate id() through.
    assert_eq!(provider.id(), "mock");

    // A basic completion should succeed.
    let resp = provider.complete(default_request()).await.unwrap();
    assert_eq!(resp.usage.input_tokens, 100);
}

// ---------------------------------------------------------------------------
// 2. Retry — retries on retryable errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_retry_provider_retries_on_retryable() {
    // Fail 2 times with 429, then succeed. Policy allows 3 retries.
    let mock = Arc::new(MockProvider::fail_then_ok(
        2,
        "HTTP 429 rate limit exceeded",
    ));
    let policy = RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(1), // fast for tests
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(
        // We need to wrap Arc<MockProvider> so we can inspect call_count later.
        // Use a thin wrapper that delegates to the Arc.
        Box::new(ArcProvider(Arc::clone(&mock))),
        policy,
    );

    let resp = provider.complete(default_request()).await;
    assert!(resp.is_ok(), "Should succeed after retries");
    assert_eq!(mock.calls(), 3); // 2 failures + 1 success
}

#[tokio::test]
async fn test_retry_provider_exhausts_retries() {
    let mock = Arc::new(MockProvider::always_fail("HTTP 429 rate limit exceeded"));
    let policy = RetryPolicy {
        max_retries: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(Box::new(ArcProvider(Arc::clone(&mock))), policy);

    let resp = provider.complete(default_request()).await;
    assert!(resp.is_err(), "Should fail after exhausting retries");
    // 1 initial + 2 retries = 3 total
    assert_eq!(mock.calls(), 3);
}

// ---------------------------------------------------------------------------
// 3. Retry — no retry on auth errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_retry_provider_no_retry_on_auth() {
    let mock = Arc::new(MockProvider::always_fail(
        "401 unauthorized api_key invalid",
    ));
    let policy = RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = RetryProvider::new(Box::new(ArcProvider(Arc::clone(&mock))), policy);

    let resp = provider.complete(default_request()).await;
    assert!(resp.is_err());
    // Auth errors are not retryable — only 1 call.
    assert_eq!(mock.calls(), 1);
}

// ---------------------------------------------------------------------------
// 4. Circuit Breaker — opens after failures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_circuit_breaker_opens_after_failures() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        reset_timeout: Duration::from_secs(60),
        success_threshold: 2,
    };

    let mock = MockProvider::always_fail("HTTP 500 internal server error");
    let cb = CircuitBreakerProvider::new(Box::new(mock), config);

    // Make 3 failing requests to trigger open state.
    for _ in 0..3 {
        let _ = cb.complete(default_request()).await;
    }

    let state = cb.check_state().await;
    assert_eq!(state, CircuitState::Open);

    // Next request should be rejected immediately with circuit breaker error.
    let err = cb.complete(default_request()).await.unwrap_err();
    assert!(
        err.to_string().contains("Circuit breaker is open"),
        "Expected circuit breaker open error, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// 5. Circuit Breaker — half-open recovery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_circuit_breaker_half_open_recovery() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        reset_timeout: Duration::from_millis(50), // very short for tests
        success_threshold: 1,
    };

    // Fail 2 times then succeed forever.
    let mock = MockProvider::fail_then_ok(2, "HTTP 500 server error");
    let cb = CircuitBreakerProvider::new(Box::new(mock), config);

    // Trigger open state.
    let _ = cb.complete(default_request()).await;
    let _ = cb.complete(default_request()).await;
    assert_eq!(cb.check_state().await, CircuitState::Open);

    // Wait for reset_timeout to elapse.
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Should transition to HalfOpen.
    let state = cb.check_state().await;
    assert_eq!(state, CircuitState::HalfOpen);

    // Next request should succeed (mock will now return Ok).
    let resp = cb.complete(default_request()).await;
    assert!(resp.is_ok(), "Should succeed in HalfOpen state");

    // Circuit should now be closed.
    let state = cb.check_state().await;
    assert_eq!(state, CircuitState::Closed);
}

// ---------------------------------------------------------------------------
// 6. Cost Guard — blocks over budget
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cost_guard_blocks_over_budget() {
    let budget = CostBudget::daily(0.0001); // very small budget
    let mock = MockProvider::always_ok().with_usage(1000, 1000);
    let cg = CostGuardProvider::new(Box::new(mock), budget);

    // First request should succeed and record cost.
    let resp = cg.complete(default_request()).await;
    assert!(resp.is_ok());

    // Budget should now be exceeded.
    let err = cg.complete(default_request()).await.unwrap_err();
    assert!(
        err.to_string().contains("Cost budget exceeded"),
        "Expected budget exceeded error, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// 7. Cost Guard — tracks spending
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cost_guard_tracks_spending() {
    let budget = CostBudget::daily(100.0); // generous budget
    let mock = MockProvider::always_ok().with_usage(1000, 1000);
    let cg = CostGuardProvider::new(Box::new(mock), budget);

    assert_eq!(cg.spent_dollars(), 0.0);

    let _ = cg.complete(default_request()).await.unwrap();

    // Expected cost: (1000 * 0.003/1000) + (1000 * 0.015/1000) = 0.003 + 0.015 = 0.018
    let spent = cg.spent_dollars();
    assert!(
        (spent - 0.018).abs() < 0.001,
        "Expected ~$0.018, got ${:.6}",
        spent
    );

    // Second request should add the same amount.
    let _ = cg.complete(default_request()).await.unwrap();
    let spent2 = cg.spent_dollars();
    assert!(
        (spent2 - 0.036).abs() < 0.001,
        "Expected ~$0.036, got ${:.6}",
        spent2
    );
}

// ---------------------------------------------------------------------------
// 8. Full Pipeline Chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_chain() {
    // Build a full pipeline: retry → circuit_breaker → cost_guard.
    let mock = MockProvider::fail_then_ok(1, "HTTP 429 rate limit exceeded").with_usage(500, 200);

    let policy = RetryPolicy {
        max_retries: 2,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(10),
        backoff_factor: 1.0,
    };

    let provider = ProviderPipelineBuilder::new(Box::new(mock))
        .with_retry(policy)
        .with_circuit_breaker(CircuitBreakerConfig {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 2,
        })
        .with_cost_guard(CostBudget::daily(10.0))
        .build();

    // The retry layer should handle the first failure transparently.
    let resp = provider.complete(default_request()).await;
    assert!(resp.is_ok(), "Pipeline should succeed: {:?}", resp.err());

    let resp = resp.unwrap();
    assert_eq!(resp.usage.input_tokens, 500);
    assert_eq!(resp.usage.output_tokens, 200);
}

// ---------------------------------------------------------------------------
// 9. Pipeline builder preserves provider id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_preserves_id() {
    let provider = ProviderPipelineBuilder::new(Box::new(MockProvider::always_ok()))
        .with_retry(RetryPolicy::default())
        .with_circuit_breaker(CircuitBreakerConfig::default())
        .build();

    assert_eq!(provider.id(), "mock");
}

// ---------------------------------------------------------------------------
// 10. Cost guard with manual cost injection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cost_guard_manual_injection() {
    let budget = CostBudget::daily(1.0);
    let mock = MockProvider::always_ok();
    let cg = CostGuardProvider::new(Box::new(mock), budget);

    // Inject $0.5 of cost.
    cg.add_cost_micros(500_000);
    assert!((cg.spent_dollars() - 0.5).abs() < 0.001);

    // Should still be under budget.
    let resp = cg.complete(default_request()).await;
    assert!(resp.is_ok());

    // Now inject enough to exceed budget.
    cg.add_cost_micros(600_000); // total > $1.0
    let err = cg.complete(default_request()).await.unwrap_err();
    assert!(err.to_string().contains("Cost budget exceeded"));
}

// ---------------------------------------------------------------------------
// Helper: ArcProvider — thin wrapper to share MockProvider via Arc
// ---------------------------------------------------------------------------

struct ArcProvider(Arc<MockProvider>);

#[async_trait]
impl Provider for ArcProvider {
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
