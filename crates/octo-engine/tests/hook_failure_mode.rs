//! Tests for HookFailureMode (FailOpen / FailClosed) behavior.

use async_trait::async_trait;
use octo_engine::{HookAction, HookContext, HookFailureMode, HookHandler, HookPoint, HookRegistry};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Test hook handlers
// ---------------------------------------------------------------------------

/// A hook that always fails, using the default failure mode (FailOpen).
struct FailOpenErrorHandler;

#[async_trait]
impl HookHandler for FailOpenErrorHandler {
    fn name(&self) -> &str {
        "fail-open-error"
    }

    async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
        anyhow::bail!("something went wrong (fail-open)")
    }
    // failure_mode() defaults to FailOpen
}

/// A hook that always fails, with FailClosed mode.
struct FailClosedErrorHandler;

#[async_trait]
impl HookHandler for FailClosedErrorHandler {
    fn name(&self) -> &str {
        "fail-closed-error"
    }

    fn failure_mode(&self) -> HookFailureMode {
        HookFailureMode::FailClosed
    }

    async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
        anyhow::bail!("critical failure (fail-closed)")
    }
}

/// A hook that succeeds with Continue.
struct SuccessHandler {
    label: &'static str,
}

#[async_trait]
impl HookHandler for SuccessHandler {
    fn name(&self) -> &str {
        self.label
    }

    async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
        Ok(HookAction::Continue)
    }
}

/// A hook that succeeds, running after a FailOpen hook to prove the chain continued.
struct TrailingSuccessHandler;

#[async_trait]
impl HookHandler for TrailingSuccessHandler {
    fn name(&self) -> &str {
        "trailing-success"
    }

    fn priority(&self) -> u32 {
        200 // runs after default-priority hooks
    }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        // Signal that this handler actually ran by modifying metadata.
        let mut new_ctx = ctx.clone();
        new_ctx.set_metadata("trailing_ran".to_string(), serde_json::Value::Bool(true));
        Ok(HookAction::Modify(new_ctx))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_default_failure_mode_is_fail_open() {
    let handler = FailOpenErrorHandler;
    assert_eq!(handler.failure_mode(), HookFailureMode::FailOpen);
}

#[tokio::test]
async fn test_fail_open_error_continues_chain() {
    let registry = HookRegistry::new();
    // Register a FailOpen handler that errors, followed by a success handler.
    registry
        .register(HookPoint::PreToolUse, Arc::new(FailOpenErrorHandler))
        .await;
    registry
        .register(HookPoint::PreToolUse, Arc::new(TrailingSuccessHandler))
        .await;

    let ctx = HookContext::new().with_session("s1");
    let action = registry.execute(HookPoint::PreToolUse, &ctx).await;

    // The trailing handler should have run and produced Modify.
    match &action {
        HookAction::Modify(new_ctx) => {
            assert_eq!(
                new_ctx.metadata.get("trailing_ran"),
                Some(&serde_json::Value::Bool(true)),
                "trailing handler must have executed after FailOpen error"
            );
        }
        other => panic!("expected Modify from trailing handler, got {:?}", other),
    }
}

#[tokio::test]
async fn test_fail_closed_error_aborts_chain() {
    let registry = HookRegistry::new();
    registry
        .register(HookPoint::PreToolUse, Arc::new(FailClosedErrorHandler))
        .await;
    // This handler should never run because the chain aborts.
    registry
        .register(HookPoint::PreToolUse, Arc::new(TrailingSuccessHandler))
        .await;

    let ctx = HookContext::new().with_session("s1");
    let action = registry.execute(HookPoint::PreToolUse, &ctx).await;

    match &action {
        HookAction::Abort(reason) => {
            assert!(
                reason.contains("FailClosed"),
                "abort reason should mention FailClosed, got: {}",
                reason
            );
            assert!(
                reason.contains("fail-closed-error"),
                "abort reason should mention handler name, got: {}",
                reason
            );
        }
        other => panic!("expected Abort from FailClosed handler, got {:?}", other),
    }
}

#[tokio::test]
async fn test_mixed_modes_fail_open_then_fail_closed() {
    // FailOpen error first (priority 100), then FailClosed error (priority 100).
    // The FailOpen error is skipped; FailClosed error triggers Abort.
    let registry = HookRegistry::new();

    // Both have default priority 100, so insertion order determines tie-breaking
    // after sort_by_key (stable sort preserves insertion order for equal keys).
    registry
        .register(HookPoint::PreTask, Arc::new(FailOpenErrorHandler))
        .await;
    registry
        .register(HookPoint::PreTask, Arc::new(FailClosedErrorHandler))
        .await;

    let ctx = HookContext::new();
    let action = registry.execute(HookPoint::PreTask, &ctx).await;

    assert!(
        matches!(&action, HookAction::Abort(reason) if reason.contains("FailClosed")),
        "FailClosed handler should abort even though FailOpen handler ran first, got {:?}",
        action
    );
}

#[tokio::test]
async fn test_mixed_modes_success_then_fail_open_then_success() {
    // success (50) → fail-open error (100) → success (200)
    // All should run; final result is Modify from trailing handler.
    let registry = HookRegistry::new();

    struct EarlySuccess;
    #[async_trait]
    impl HookHandler for EarlySuccess {
        fn name(&self) -> &str {
            "early-success"
        }
        fn priority(&self) -> u32 {
            50
        }
        async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
            Ok(HookAction::Continue)
        }
    }

    registry
        .register(HookPoint::PostTask, Arc::new(EarlySuccess))
        .await;
    registry
        .register(HookPoint::PostTask, Arc::new(FailOpenErrorHandler))
        .await;
    registry
        .register(HookPoint::PostTask, Arc::new(TrailingSuccessHandler))
        .await;

    let ctx = HookContext::new();
    let action = registry.execute(HookPoint::PostTask, &ctx).await;

    match &action {
        HookAction::Modify(new_ctx) => {
            assert_eq!(
                new_ctx.metadata.get("trailing_ran"),
                Some(&serde_json::Value::Bool(true)),
            );
        }
        other => panic!("expected Modify, got {:?}", other),
    }
}

#[tokio::test]
async fn test_fail_closed_success_does_not_abort() {
    // A FailClosed handler that succeeds should NOT abort.
    struct FailClosedSuccessHandler;
    #[async_trait]
    impl HookHandler for FailClosedSuccessHandler {
        fn name(&self) -> &str {
            "fail-closed-success"
        }
        fn failure_mode(&self) -> HookFailureMode {
            HookFailureMode::FailClosed
        }
        async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
            Ok(HookAction::Continue)
        }
    }

    let registry = HookRegistry::new();
    registry
        .register(HookPoint::PreToolUse, Arc::new(FailClosedSuccessHandler))
        .await;

    let ctx = HookContext::new();
    let action = registry.execute(HookPoint::PreToolUse, &ctx).await;

    assert!(
        matches!(action, HookAction::Continue),
        "FailClosed handler that succeeds should not abort, got {:?}",
        action
    );
}
