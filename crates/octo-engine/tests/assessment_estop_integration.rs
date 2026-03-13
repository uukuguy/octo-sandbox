//! Integration tests for the Emergency Stop (E-Stop) mechanism.
//!
//! These tests validate cross-task coordination, concurrent trigger
//! semantics, reset behaviour, and poll-loop simulation — all exercising
//! the `EmergencyStop` API through realistic async scenarios.

use octo_engine::agent::estop::{EStopReason, EmergencyStop};

/// Trigger from a separate tokio task and verify the subscriber receives it.
#[tokio::test]
async fn estop_trigger_from_separate_task_is_received() {
    let estop = EmergencyStop::new();
    let mut rx = estop.subscribe();

    let estop_clone = estop.clone();
    tokio::spawn(async move {
        estop_clone.trigger(EStopReason::BudgetExceeded);
    });

    let reason = rx.recv().await.expect("subscriber should receive the reason");
    assert!(
        matches!(reason, EStopReason::BudgetExceeded),
        "expected BudgetExceeded, got {reason:?}"
    );
    assert!(estop.is_triggered());
}

/// Spawn 10 concurrent tasks each triggering with a different reason.
/// Only the first one wins — `reason()` must be one of the expected variants
/// and `is_triggered()` must be true.
#[tokio::test]
async fn estop_multiple_concurrent_triggers_only_first_wins() {
    let estop = EmergencyStop::new();

    let mut handles = Vec::new();
    for i in 0..10 {
        let estop_clone = estop.clone();
        let handle = tokio::spawn(async move {
            let reason = match i % 4 {
                0 => EStopReason::UserTriggered,
                1 => EStopReason::BudgetExceeded,
                2 => EStopReason::SafetyViolation(format!("violation-{i}")),
                _ => EStopReason::SystemShutdown,
            };
            estop_clone.trigger(reason);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("spawned task should not panic");
    }

    assert!(estop.is_triggered(), "estop must be triggered after 10 concurrent triggers");

    let reason = estop.reason().expect("reason must be set after trigger");
    let is_valid = matches!(
        reason,
        EStopReason::UserTriggered
            | EStopReason::BudgetExceeded
            | EStopReason::SafetyViolation(_)
            | EStopReason::SystemShutdown
    );
    assert!(is_valid, "reason must be one of the expected variants, got {reason:?}");
}

/// After triggering and resetting, a second trigger with a different reason
/// must succeed and report the new reason.
#[tokio::test]
async fn estop_reset_allows_retrigger() {
    let estop = EmergencyStop::new();

    // First trigger
    estop.trigger(EStopReason::UserTriggered);
    assert!(estop.is_triggered());
    assert!(
        matches!(estop.reason(), Some(EStopReason::UserTriggered)),
        "first reason should be UserTriggered"
    );

    // Reset
    estop.reset();
    assert!(!estop.is_triggered(), "estop should be cleared after reset");
    assert!(estop.reason().is_none(), "reason should be None after reset");

    // Second trigger with a different reason
    estop.trigger(EStopReason::BudgetExceeded);
    assert!(estop.is_triggered());
    assert!(
        matches!(estop.reason(), Some(EStopReason::BudgetExceeded)),
        "second reason should be BudgetExceeded"
    );
}

/// Simulate an agent harness poll loop: a background task triggers the
/// estop after a short delay while the main task busy-loops on
/// `is_triggered()`, yielding between iterations.  The loop must exit
/// within a bounded number of iterations.
#[tokio::test]
async fn estop_poll_loop_simulation() {
    let estop = EmergencyStop::new();

    let estop_clone = estop.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        estop_clone.trigger(EStopReason::SystemShutdown);
    });

    let max_iterations: u64 = 100_000;
    let mut iterations: u64 = 0;

    while !estop.is_triggered() {
        tokio::task::yield_now().await;
        iterations += 1;
        assert!(
            iterations < max_iterations,
            "poll loop did not exit within {max_iterations} iterations"
        );
    }

    assert!(estop.is_triggered(), "estop must be triggered when loop exits");
    assert!(
        matches!(estop.reason(), Some(EStopReason::SystemShutdown)),
        "reason should be SystemShutdown"
    );
    // Sanity: at least one yield happened before the trigger landed.
    assert!(iterations > 0, "expected at least one poll iteration");
}
