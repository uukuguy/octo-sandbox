//! AV-T4: Tests for unattended retry mode.

use std::time::Duration;
use octo_engine::providers::retry::{LlmErrorKind, RetryInfo, RetryPolicy};

#[test]
fn test_unattended_retries_indefinitely_for_rate_limit() {
    let policy = RetryPolicy::unattended();
    let info = RetryInfo {
        kind: LlmErrorKind::RateLimit,
        retry_after: None,
        error_code: None,
    };
    // Should retry at attempt 100
    let delay = policy.should_retry_with_info(&info, 100);
    assert!(delay.is_some(), "Unattended should retry RateLimit indefinitely");
    // Delay capped at unattended_max_delay (5 min)
    assert!(delay.unwrap() <= Duration::from_secs(300));
}

#[test]
fn test_unattended_retries_overloaded() {
    let policy = RetryPolicy::unattended();
    let info = RetryInfo {
        kind: LlmErrorKind::Overloaded,
        retry_after: None,
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 50);
    assert!(delay.is_some(), "Unattended should retry Overloaded");
}

#[test]
fn test_unattended_does_not_retry_auth_error() {
    let policy = RetryPolicy::unattended();
    let info = RetryInfo {
        kind: LlmErrorKind::AuthError,
        retry_after: None,
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 0);
    assert!(delay.is_none(), "Auth errors should never retry");
}

#[test]
fn test_unattended_does_not_retry_billing_error() {
    let policy = RetryPolicy::unattended();
    let info = RetryInfo {
        kind: LlmErrorKind::BillingError,
        retry_after: None,
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 0);
    assert!(delay.is_none(), "Billing errors should never retry");
}

#[test]
fn test_normal_policy_stops_at_max_retries() {
    let policy = RetryPolicy::default();
    let info = RetryInfo {
        kind: LlmErrorKind::RateLimit,
        retry_after: None,
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 3);
    assert!(delay.is_none(), "Normal policy should stop at max_retries=3");
}

#[test]
fn test_normal_policy_retries_before_max() {
    let policy = RetryPolicy::default();
    let info = RetryInfo {
        kind: LlmErrorKind::RateLimit,
        retry_after: None,
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 2);
    assert!(delay.is_some(), "Normal policy should retry before max_retries");
}

#[test]
fn test_unattended_respects_retry_after_header() {
    let policy = RetryPolicy::unattended();
    let info = RetryInfo {
        kind: LlmErrorKind::RateLimit,
        retry_after: Some(Duration::from_secs(10)),
        error_code: None,
    };
    let delay = policy.should_retry_with_info(&info, 0);
    assert_eq!(delay, Some(Duration::from_secs(10)));
}

#[test]
fn test_unattended_str_retries_rate_limit() {
    let policy = RetryPolicy::unattended();
    assert!(policy.should_retry_str("http 429 rate limit exceeded", 1000));
}

#[test]
fn test_normal_str_stops_at_max() {
    let policy = RetryPolicy::default();
    assert!(!policy.should_retry_str("http 429 rate limit exceeded", 3));
}
