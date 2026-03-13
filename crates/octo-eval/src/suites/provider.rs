//! Provider fault-tolerance evaluation suite.
//!
//! Tests `ProviderChain` failover, retry, and degradation behavior using
//! `FaultProvider` — a mock provider that replays a configurable sequence
//! of successes, errors, and delays.
//!
//! This suite does NOT use the agent loop or `EvalRunner`.  It exercises
//! `octo-engine`'s Provider layer directly and returns an `EvalReport`
//! for integration with the reporting system.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use octo_engine::providers::chain::{FailoverPolicy, LlmInstance, ProviderChain};
use octo_engine::providers::traits::{CompletionStream, Provider};
use octo_types::message::ContentBlock;
use octo_types::provider::{CompletionResponse, StopReason, TokenUsage};
use octo_types::CompletionRequest;

use crate::runner::{EvalReport, TaskResult};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::AgentOutput;

// ---------------------------------------------------------------------------
// FaultProvider — deterministic mock provider
// ---------------------------------------------------------------------------

/// A response or error that `FaultProvider` replays in order.
#[derive(Clone, Debug)]
pub enum FaultAction {
    /// Return a successful completion with the given text.
    Success(String),
    /// Return an error with this message.
    Error(String),
    /// Sleep for `ms` then execute the inner action (one level only).
    Delay { ms: u64, then: Box<FaultAction> },
}

/// Mock provider that replays a pre-configured action sequence.
///
/// When the cursor exceeds the action list, the **last** action is repeated.
pub struct FaultProvider {
    name: String,
    actions: Vec<FaultAction>,
    cursor: AtomicUsize,
}

impl FaultProvider {
    pub fn new(name: impl Into<String>, actions: Vec<FaultAction>) -> Self {
        Self {
            name: name.into(),
            actions,
            cursor: AtomicUsize::new(0),
        }
    }

    /// Number of times `complete` has been called.
    pub fn call_count(&self) -> usize {
        self.cursor.load(Ordering::Relaxed)
    }

    /// Execute a single `FaultAction`, returning a `CompletionResponse` or error.
    async fn execute_action(&self, action: &FaultAction) -> Result<CompletionResponse> {
        match action {
            FaultAction::Success(text) => Ok(Self::text_response(text)),
            FaultAction::Error(msg) => Err(anyhow::anyhow!("{}", msg)),
            FaultAction::Delay { ms, then } => {
                tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                match then.as_ref() {
                    FaultAction::Success(text) => Ok(Self::text_response(text)),
                    FaultAction::Error(msg) => Err(anyhow::anyhow!("{}", msg)),
                    FaultAction::Delay { .. } => {
                        Err(anyhow::anyhow!("Nested Delay not supported"))
                    }
                }
            }
        }
    }

    fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            id: format!("fault-{}", uuid::Uuid::new_v4()),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        }
    }
}

#[async_trait]
impl Provider for FaultProvider {
    fn id(&self) -> &str {
        &self.name
    }

    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed);
        let action = self
            .actions
            .get(idx)
            .or_else(|| self.actions.last())
            .cloned()
            .unwrap_or(FaultAction::Error("No actions configured".into()));
        self.execute_action(&action).await
    }

    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream> {
        Err(anyhow::anyhow!("FaultProvider does not support streaming"))
    }
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

fn default_request() -> CompletionRequest {
    CompletionRequest::default()
}

fn pass(task_id: &str, behavior: &str, duration_ms: u64) -> TaskResult {
    TaskResult {
        task_id: task_id.into(),
        output: AgentOutput::default(),
        score: EvalScore::pass(
            1.0,
            ScoreDetails::BehaviorCheck {
                expected_behavior: behavior.into(),
                observed: true,
            },
        ),
        duration_ms,
    }
}

fn fail(task_id: &str, behavior: &str, duration_ms: u64) -> TaskResult {
    TaskResult {
        task_id: task_id.into(),
        output: AgentOutput::default(),
        score: EvalScore::fail(
            0.0,
            ScoreDetails::BehaviorCheck {
                expected_behavior: behavior.into(),
                observed: false,
            },
        ),
        duration_ms,
    }
}

fn extract_text(resp: &CompletionResponse) -> String {
    resp.content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Build a `ProviderChain` (Automatic policy) with pre-configured `LlmInstance`
/// entries.  The chain does NOT actually create real providers — `ChainProvider`
/// creates them via `create_provider`, which would need real API keys.
///
/// Instead, for tests that need direct failover, we call `FaultProvider` objects
/// directly and use `ProviderChain` only for the instance management tests.
fn make_instance(id: &str, priority: u8) -> LlmInstance {
    LlmInstance {
        id: id.to_string(),
        provider: "anthropic".to_string(),
        api_key: "fake-key".to_string(),
        base_url: None,
        model: "test-model".to_string(),
        priority,
        max_rpm: None,
        enabled: true,
    }
}

// ---------------------------------------------------------------------------
// ProviderSuite
// ---------------------------------------------------------------------------

/// Provider fault-tolerance evaluation suite.
pub struct ProviderSuite;

impl ProviderSuite {
    /// Run all provider fault-tolerance tests and return an `EvalReport`.
    pub async fn run() -> Result<EvalReport> {
        let mut results = Vec::new();

        results.push(Self::test_single_provider_success().await);
        results.push(Self::test_fault_provider_error_then_success().await);
        results.push(Self::test_failover_basic().await);
        results.push(Self::test_retry_success().await);
        results.push(Self::test_all_fail_graceful().await);
        results.push(Self::test_intermittent_recovery().await);
        results.push(Self::test_timeout_then_success().await);
        results.push(Self::test_chain_priority_order().await);
        results.push(Self::test_chain_mark_unhealthy_skips().await);
        results.push(Self::test_data_consistency_after_failover().await);

        Ok(EvalReport::from_results(results))
    }

    // -- Test 1: Single provider baseline --------------------------------

    async fn test_single_provider_success() -> TaskResult {
        let id = "prov-01-baseline";
        let behavior = "Single healthy provider returns success";
        let start = Instant::now();

        let provider = FaultProvider::new("p1", vec![FaultAction::Success("hello".into())]);
        match provider.complete(default_request()).await {
            Ok(resp) if extract_text(&resp) == "hello" => {
                pass(id, behavior, start.elapsed().as_millis() as u64)
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 2: Error then success on same provider ---------------------

    async fn test_fault_provider_error_then_success() -> TaskResult {
        let id = "prov-02-error-then-success";
        let behavior = "FaultProvider error on first call, success on second";
        let start = Instant::now();

        let provider = FaultProvider::new(
            "p1",
            vec![
                FaultAction::Error("transient".into()),
                FaultAction::Success("recovered".into()),
            ],
        );

        let first = provider.complete(default_request()).await;
        let second = provider.complete(default_request()).await;

        if first.is_err()
            && second.is_ok()
            && extract_text(&second.unwrap()) == "recovered"
            && provider.call_count() == 2
        {
            pass(id, behavior, start.elapsed().as_millis() as u64)
        } else {
            fail(id, behavior, start.elapsed().as_millis() as u64)
        }
    }

    // -- Test 3: Primary fails, secondary succeeds (manual failover) -----

    async fn test_failover_basic() -> TaskResult {
        let id = "prov-03-failover-basic";
        let behavior = "Primary fails, fallback to secondary succeeds";
        let start = Instant::now();

        let primary = FaultProvider::new("primary", vec![FaultAction::Error("500".into())]);
        let secondary =
            FaultProvider::new("secondary", vec![FaultAction::Success("backup-ok".into())]);

        // Simulate manual failover: try primary, on error try secondary
        let result = match primary.complete(default_request()).await {
            Ok(resp) => Ok(resp),
            Err(_) => secondary.complete(default_request()).await,
        };

        match result {
            Ok(resp) if extract_text(&resp) == "backup-ok" => {
                pass(id, behavior, start.elapsed().as_millis() as u64)
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 4: Retry succeeds after transient failure ------------------

    async fn test_retry_success() -> TaskResult {
        let id = "prov-04-retry-success";
        let behavior = "Retry loop succeeds after 2 transient failures";
        let start = Instant::now();

        let provider = FaultProvider::new(
            "p1",
            vec![
                FaultAction::Error("fail-1".into()),
                FaultAction::Error("fail-2".into()),
                FaultAction::Success("ok-3".into()),
            ],
        );

        // Simple retry loop (max 5 attempts)
        let mut result = Err(anyhow::anyhow!("not started"));
        for _ in 0..5 {
            result = provider.complete(default_request()).await;
            if result.is_ok() {
                break;
            }
        }

        match result {
            Ok(resp)
                if extract_text(&resp) == "ok-3" && provider.call_count() == 3 =>
            {
                pass(id, behavior, start.elapsed().as_millis() as u64)
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 5: All providers fail — graceful degradation ---------------

    async fn test_all_fail_graceful() -> TaskResult {
        let id = "prov-05-all-fail";
        let behavior = "All providers fail, error is propagated gracefully";
        let start = Instant::now();

        let p1 = FaultProvider::new("p1", vec![FaultAction::Error("p1-down".into())]);
        let p2 = FaultProvider::new("p2", vec![FaultAction::Error("p2-down".into())]);
        let p3 = FaultProvider::new("p3", vec![FaultAction::Error("p3-down".into())]);

        let providers: Vec<&FaultProvider> = vec![&p1, &p2, &p3];
        let mut last_err = None;
        for p in providers {
            match p.complete(default_request()).await {
                Ok(_resp) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        if last_err.is_some() {
            // All failed — that IS the expected behavior
            pass(id, behavior, start.elapsed().as_millis() as u64)
        } else {
            fail(id, behavior, start.elapsed().as_millis() as u64)
        }
    }

    // -- Test 6: Intermittent failure recovery ---------------------------

    async fn test_intermittent_recovery() -> TaskResult {
        let id = "prov-06-intermittent";
        let behavior = "Provider alternates fail/success, all successes captured";
        let start = Instant::now();

        let provider = FaultProvider::new(
            "flaky",
            vec![
                FaultAction::Success("ok-1".into()),
                FaultAction::Error("blip".into()),
                FaultAction::Success("ok-2".into()),
                FaultAction::Error("blip".into()),
                FaultAction::Success("ok-3".into()),
            ],
        );

        let mut successes = 0u32;
        for _ in 0..5 {
            if provider.complete(default_request()).await.is_ok() {
                successes += 1;
            }
        }

        if successes == 3 && provider.call_count() == 5 {
            pass(id, behavior, start.elapsed().as_millis() as u64)
        } else {
            fail(id, behavior, start.elapsed().as_millis() as u64)
        }
    }

    // -- Test 7: Delay (simulated timeout) then success ------------------

    async fn test_timeout_then_success() -> TaskResult {
        let id = "prov-07-timeout-then-success";
        let behavior = "Delayed response still returns success";
        let start = Instant::now();

        let provider = FaultProvider::new(
            "slow",
            vec![FaultAction::Delay {
                ms: 50,
                then: Box::new(FaultAction::Success("slow-ok".into())),
            }],
        );

        match provider.complete(default_request()).await {
            Ok(resp) if extract_text(&resp) == "slow-ok" => {
                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed >= 40 {
                    // confirm the delay actually happened
                    pass(id, behavior, elapsed)
                } else {
                    fail(id, behavior, elapsed)
                }
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 8: ProviderChain returns instances in priority order --------

    async fn test_chain_priority_order() -> TaskResult {
        let id = "prov-08-chain-priority";
        let behavior = "ProviderChain returns lowest-priority instance first";
        let start = Instant::now();

        let chain = ProviderChain::new(FailoverPolicy::Automatic);
        chain.add_instance(make_instance("high", 10)).await;
        chain.add_instance(make_instance("low", 1)).await;
        chain.add_instance(make_instance("mid", 5)).await;

        match chain.get_available().await {
            Ok(inst) if inst.id == "low" => {
                pass(id, behavior, start.elapsed().as_millis() as u64)
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 9: mark_unhealthy causes skip ------------------------------

    async fn test_chain_mark_unhealthy_skips() -> TaskResult {
        let id = "prov-09-unhealthy-skip";
        let behavior = "Unhealthy instance is skipped, next healthy chosen";
        let start = Instant::now();

        let chain = ProviderChain::new(FailoverPolicy::Automatic);
        chain.add_instance(make_instance("a", 0)).await;
        chain.add_instance(make_instance("b", 1)).await;

        chain.mark_unhealthy("a", "simulated failure").await;

        match chain.get_available().await {
            Ok(inst) if inst.id == "b" => {
                pass(id, behavior, start.elapsed().as_millis() as u64)
            }
            _ => fail(id, behavior, start.elapsed().as_millis() as u64),
        }
    }

    // -- Test 10: Data consistency after failover -----------------------

    async fn test_data_consistency_after_failover() -> TaskResult {
        let id = "prov-10-data-consistency";
        let behavior = "Response from fallback provider has valid structure";
        let start = Instant::now();

        let primary = FaultProvider::new("primary", vec![FaultAction::Error("down".into())]);
        let backup = FaultProvider::new(
            "backup",
            vec![FaultAction::Success("consistent-data".into())],
        );

        let resp = match primary.complete(default_request()).await {
            Ok(r) => r,
            Err(_) => backup.complete(default_request()).await.unwrap(),
        };

        // Verify structural integrity
        let text = extract_text(&resp);
        let has_id = !resp.id.is_empty();
        let has_stop = resp.stop_reason.is_some();
        let text_ok = text == "consistent-data";

        if has_id && has_stop && text_ok {
            pass(id, behavior, start.elapsed().as_millis() as u64)
        } else {
            fail(id, behavior, start.elapsed().as_millis() as u64)
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fault_provider_success() {
        let p = FaultProvider::new("t", vec![FaultAction::Success("hi".into())]);
        let resp = p.complete(default_request()).await.unwrap();
        assert_eq!(extract_text(&resp), "hi");
        assert_eq!(p.call_count(), 1);
    }

    #[tokio::test]
    async fn test_fault_provider_error() {
        let p = FaultProvider::new("t", vec![FaultAction::Error("boom".into())]);
        let err = p.complete(default_request()).await.unwrap_err();
        assert!(err.to_string().contains("boom"));
        assert_eq!(p.call_count(), 1);
    }

    #[tokio::test]
    async fn test_fault_provider_sequence() {
        let p = FaultProvider::new(
            "t",
            vec![
                FaultAction::Error("e1".into()),
                FaultAction::Success("ok".into()),
            ],
        );
        assert!(p.complete(default_request()).await.is_err());
        assert!(p.complete(default_request()).await.is_ok());
        assert_eq!(p.call_count(), 2);
    }

    #[tokio::test]
    async fn test_fault_provider_repeats_last() {
        let p = FaultProvider::new("t", vec![FaultAction::Success("x".into())]);
        for _ in 0..5 {
            assert!(p.complete(default_request()).await.is_ok());
        }
        assert_eq!(p.call_count(), 5);
    }

    #[tokio::test]
    async fn test_fault_provider_delay() {
        let p = FaultProvider::new(
            "t",
            vec![FaultAction::Delay {
                ms: 20,
                then: Box::new(FaultAction::Success("delayed".into())),
            }],
        );
        let start = Instant::now();
        let resp = p.complete(default_request()).await.unwrap();
        assert!(start.elapsed().as_millis() >= 15);
        assert_eq!(extract_text(&resp), "delayed");
    }

    #[tokio::test]
    async fn test_provider_suite_runs() {
        let report = ProviderSuite::run().await.unwrap();
        assert_eq!(report.total, 10);
        // All tests in this suite use deterministic mocks — they should all pass
        assert_eq!(
            report.passed, 10,
            "Expected all 10 provider tests to pass, got {}/10. Failures: {:?}",
            report.passed,
            report
                .results
                .iter()
                .filter(|r| !r.score.passed)
                .map(|r| &r.task_id)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_chain_no_healthy_instances() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);
        assert!(chain.get_available().await.is_err());
    }

    #[tokio::test]
    async fn test_chain_disabled_instance_skipped() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);
        let mut inst = make_instance("dis", 0);
        inst.enabled = false;
        chain.add_instance(inst).await;
        assert!(chain.get_available().await.is_err());
    }
}
