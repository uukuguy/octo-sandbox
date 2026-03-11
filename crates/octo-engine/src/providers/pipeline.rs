//! Provider pipeline builder — decorator chain pattern for Provider trait.
//!
//! Enables composing providers with cross-cutting concerns:
//! Raw → Retry → CircuitBreaker → CostGuard → Recording
//!
//! # Example
//! ```ignore
//! let provider = ProviderPipelineBuilder::new(raw_provider)
//!     .with_retry(RetryPolicy::default())
//!     .with_circuit_breaker(CircuitBreakerConfig::default())
//!     .with_cost_guard(CostBudget::daily(10.0))
//!     .build();
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use octo_types::{CompletionRequest, CompletionResponse};

use super::response_cache::ResponseCacheProvider;
use super::retry::{LlmErrorKind, RetryPolicy};
use super::smart_router::{QueryAnalyzer, QueryComplexity, SmartRouterProvider};
use super::traits::{CompletionStream, Provider};
use super::usage_recorder::{UsageRecorderProvider, UsageStats};

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// Circuit breaker state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Failing — requests are rejected immediately.
    Open,
    /// Testing recovery — a limited number of requests pass through.
    HalfOpen,
}

/// Configuration for the circuit breaker decorator.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit opens (default 5).
    pub failure_threshold: u32,
    /// Duration to wait before transitioning from Open to HalfOpen (default 30s).
    pub reset_timeout: Duration,
    /// Number of successes in HalfOpen required to close the circuit (default 2).
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Provider decorator that implements the circuit breaker pattern.
pub struct CircuitBreakerProvider {
    inner: Box<dyn Provider>,
    config: CircuitBreakerConfig,
    state: RwLock<CircuitState>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_time: RwLock<Option<Instant>>,
}

impl CircuitBreakerProvider {
    pub fn new(inner: Box<dyn Provider>, config: CircuitBreakerConfig) -> Self {
        Self {
            inner,
            config,
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_time: RwLock::new(None),
        }
    }

    /// Retrieve the current circuit state, potentially transitioning Open → HalfOpen.
    pub async fn check_state(&self) -> CircuitState {
        let state = self.state.read().await;
        match *state {
            CircuitState::Open => {
                let last = self.last_failure_time.read().await;
                if let Some(t) = *last {
                    if t.elapsed() >= self.config.reset_timeout {
                        drop(last);
                        drop(state);
                        let mut s = self.state.write().await;
                        // Double-check after acquiring write lock.
                        if *s == CircuitState::Open {
                            *s = CircuitState::HalfOpen;
                            self.success_count.store(0, Ordering::SeqCst);
                            debug!("Circuit breaker transitioning to HalfOpen");
                        }
                        return s.clone();
                    }
                }
                CircuitState::Open
            }
            ref s => s.clone(),
        }
    }

    async fn on_success(&self) {
        let state = self.state.read().await;
        match *state {
            CircuitState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= self.config.success_threshold as u64 {
                    drop(state);
                    let mut s = self.state.write().await;
                    *s = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    debug!("Circuit breaker closed after {} successes", count);
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    async fn on_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut last = self.last_failure_time.write().await;
            *last = Some(Instant::now());
        }

        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => {
                if count >= self.config.failure_threshold as u64 {
                    drop(state);
                    let mut s = self.state.write().await;
                    *s = CircuitState::Open;
                    warn!("Circuit breaker opened after {} failures", count);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen immediately re-opens.
                drop(state);
                let mut s = self.state.write().await;
                *s = CircuitState::Open;
                warn!("Circuit breaker re-opened from HalfOpen on failure");
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Provider for CircuitBreakerProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let state = self.check_state().await;
        if state == CircuitState::Open {
            return Err(anyhow::anyhow!(
                "Circuit breaker is open for provider '{}'",
                self.id()
            ));
        }
        match self.inner.complete(request).await {
            Ok(resp) => {
                self.on_success().await;
                Ok(resp)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let state = self.check_state().await;
        if state == CircuitState::Open {
            return Err(anyhow::anyhow!(
                "Circuit breaker is open for provider '{}'",
                self.id()
            ));
        }
        match self.inner.stream(request).await {
            Ok(s) => {
                self.on_success().await;
                Ok(s)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cost Guard
// ---------------------------------------------------------------------------

/// Cost budget configuration for a provider.
#[derive(Debug, Clone)]
pub struct CostBudget {
    /// Maximum cost in dollars for the budget period.
    pub max_cost: f64,
    /// Budget period label (for logging/error messages).
    pub period: String,
}

impl CostBudget {
    /// Create a daily cost budget.
    pub fn daily(max_cost: f64) -> Self {
        Self {
            max_cost,
            period: "daily".to_string(),
        }
    }

    /// Create a monthly cost budget.
    pub fn monthly(max_cost: f64) -> Self {
        Self {
            max_cost,
            period: "monthly".to_string(),
        }
    }
}

/// Provider decorator that enforces a cost budget.
///
/// Costs are tracked in microdollars (1e-6 USD) using atomics for lock-free updates.
pub struct CostGuardProvider {
    inner: Box<dyn Provider>,
    budget: CostBudget,
    /// Accumulated cost in microdollars (1 microdollar = $0.000001).
    spent: AtomicU64,
}

impl CostGuardProvider {
    pub fn new(inner: Box<dyn Provider>, budget: CostBudget) -> Self {
        Self {
            inner,
            budget,
            spent: AtomicU64::new(0),
        }
    }

    fn check_budget(&self) -> Result<()> {
        let spent_micros = self.spent.load(Ordering::SeqCst);
        let spent_dollars = spent_micros as f64 / 1_000_000.0;
        if spent_dollars >= self.budget.max_cost {
            return Err(anyhow::anyhow!(
                "Cost budget exceeded: ${:.4} / ${:.2} ({})",
                spent_dollars,
                self.budget.max_cost,
                self.budget.period
            ));
        }
        Ok(())
    }

    fn record_cost(&self, response: &CompletionResponse) {
        // Estimate cost based on token usage.
        // Default rates: ~$0.003/1K input tokens, ~$0.015/1K output tokens.
        let input_cost = response.usage.input_tokens as f64 * 0.003 / 1000.0;
        let output_cost = response.usage.output_tokens as f64 * 0.015 / 1000.0;
        let cost_micros = ((input_cost + output_cost) * 1_000_000.0) as u64;
        self.spent.fetch_add(cost_micros, Ordering::SeqCst);
    }

    /// Get current spent amount in dollars.
    pub fn spent_dollars(&self) -> f64 {
        self.spent.load(Ordering::SeqCst) as f64 / 1_000_000.0
    }

    /// Manually add cost in microdollars (useful for testing or external cost tracking).
    pub fn add_cost_micros(&self, micros: u64) {
        self.spent.fetch_add(micros, Ordering::SeqCst);
    }
}

#[async_trait]
impl Provider for CostGuardProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.check_budget()?;
        let resp = self.inner.complete(request).await?;
        self.record_cost(&resp);
        Ok(resp)
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        self.check_budget()?;
        self.inner.stream(request).await
    }
}

// ---------------------------------------------------------------------------
// Retry Decorator
// ---------------------------------------------------------------------------

/// Provider decorator that retries transient failures with exponential backoff.
pub struct RetryProvider {
    inner: Box<dyn Provider>,
    policy: RetryPolicy,
}

impl RetryProvider {
    pub fn new(inner: Box<dyn Provider>, policy: RetryPolicy) -> Self {
        Self { inner, policy }
    }
}

#[async_trait]
impl Provider for RetryProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let mut last_err = None;
        for attempt in 0..=self.policy.max_retries {
            match self.inner.complete(request.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let kind = LlmErrorKind::classify_from_str(&e.to_string().to_lowercase());
                    if !kind.is_retryable() || attempt == self.policy.max_retries {
                        return Err(e);
                    }
                    let delay = self.policy.delay_for(attempt);
                    debug!("Retry attempt {} after {:?}: {}", attempt + 1, delay, e);
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Retry exhausted")))
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        // Stream does not retry — delegate directly.
        self.inner.stream(request).await
    }
}

// ---------------------------------------------------------------------------
// Pipeline Builder
// ---------------------------------------------------------------------------

/// Builder for composing provider decorators in a pipeline.
///
/// Decorators are applied inside-out: the last added decorator is the outermost layer.
/// The recommended order is: Raw → Retry → CircuitBreaker → CostGuard
///
/// # Example
/// ```ignore
/// let provider = ProviderPipelineBuilder::new(raw_provider)
///     .with_retry(RetryPolicy::default())
///     .with_circuit_breaker(CircuitBreakerConfig::default())
///     .with_cost_guard(CostBudget::daily(10.0))
///     .build();
/// ```
pub struct ProviderPipelineBuilder {
    provider: Box<dyn Provider>,
}

impl ProviderPipelineBuilder {
    /// Create a new builder wrapping the given base provider.
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self { provider }
    }

    /// Add retry with the given policy (wraps the current provider).
    pub fn with_retry(self, policy: RetryPolicy) -> Self {
        Self {
            provider: Box::new(RetryProvider::new(self.provider, policy)),
        }
    }

    /// Add circuit breaker with the given configuration.
    pub fn with_circuit_breaker(self, config: CircuitBreakerConfig) -> Self {
        Self {
            provider: Box::new(CircuitBreakerProvider::new(self.provider, config)),
        }
    }

    /// Add cost guard with the given budget.
    pub fn with_cost_guard(self, budget: CostBudget) -> Self {
        Self {
            provider: Box::new(CostGuardProvider::new(self.provider, budget)),
        }
    }

    /// Add response cache with given capacity and TTL in seconds.
    pub fn with_response_cache(self, capacity: usize, ttl_secs: u64) -> Self {
        Self {
            provider: Box::new(ResponseCacheProvider::new(
                self.provider,
                capacity,
                Duration::from_secs(ttl_secs),
            )),
        }
    }

    /// Add smart routing that overrides the model based on query complexity.
    ///
    /// The analyzer classifies requests as Simple/Medium/Complex and the
    /// `tier_models` map selects the corresponding model name. Requests whose
    /// complexity tier is not in the map fall back to `default_model`.
    ///
    /// Recommended pipeline order: Raw -> SmartRouter -> Retry -> CircuitBreaker -> CostGuard
    pub fn with_smart_routing(
        self,
        analyzer: QueryAnalyzer,
        tier_models: std::collections::HashMap<QueryComplexity, String>,
        default_model: String,
    ) -> Self {
        Self {
            provider: Box::new(SmartRouterProvider::new(
                self.provider,
                analyzer,
                tier_models,
                default_model,
            )),
        }
    }

    /// Add usage recorder (returns a handle to the shared stats).
    pub fn with_usage_recorder(self) -> (Self, Arc<RwLock<UsageStats>>) {
        let stats = Arc::new(RwLock::new(UsageStats::default()));
        let builder = Self {
            provider: Box::new(UsageRecorderProvider::with_shared_stats(
                self.provider,
                Arc::clone(&stats),
            )),
        };
        (builder, stats)
    }

    /// Build the final composed provider.
    pub fn build(self) -> Box<dyn Provider> {
        self.provider
    }
}
