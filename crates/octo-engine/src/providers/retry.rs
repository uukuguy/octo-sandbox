use std::time::Duration;

/// Error routing strategy (moltis ProviderErrorKind pattern)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorStrategy {
    /// Retry with exponential backoff (same provider)
    Retry,
    /// Failover to backup provider
    Failover,
    /// Compact context then retry
    CompactAndRetry,
    /// Fail immediately — do not retry or failover
    Fail,
}

/// LLM 错误 8 分类（参考 ARCHITECTURE_DESIGN.md §E-07）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmErrorKind {
    // === 可重试 ===
    /// HTTP 429 - 速率限制
    RateLimit,
    /// HTTP 529 / "overloaded" - 服务过载
    Overloaded,
    /// 网络超时或连接断开
    Timeout,
    /// HTTP 500/502/503 - 瞬时服务错误
    ServiceError,

    // === 不可重试 ===
    /// HTTP 402 / "credit_balance_too_low" - 账户余额不足
    BillingError,
    /// HTTP 401/403 - 认证/授权失败
    AuthError,
    /// 上下文窗口超限
    ContextOverflow,
    /// 其他未分类错误
    Unknown,
}

impl LlmErrorKind {
    /// 判断是否应该重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimit | Self::Overloaded | Self::Timeout | Self::ServiceError
        )
    }

    /// Whether this error type should trigger failover to a backup provider.
    /// Only transient service issues trigger failover — NOT auth/billing/context errors.
    pub fn should_failover(&self) -> bool {
        matches!(
            self,
            Self::RateLimit | Self::ServiceError | Self::Overloaded | Self::Unknown
        )
    }

    /// Determine the routing strategy for this error type.
    /// This enables semantic error routing: each error kind maps to a specific recovery action.
    pub fn routing_strategy(&self) -> ErrorStrategy {
        match self {
            Self::RateLimit | Self::Overloaded => ErrorStrategy::Retry,
            Self::ServiceError | Self::Timeout => ErrorStrategy::Failover,
            Self::ContextOverflow => ErrorStrategy::CompactAndRetry,
            Self::AuthError | Self::BillingError => ErrorStrategy::Fail,
            Self::Unknown => ErrorStrategy::Retry,
        }
    }

    /// 从错误消息中分类（传入 error.to_string().to_lowercase()）
    pub fn classify_from_str(msg: &str) -> Self {
        if msg.contains("429") || msg.contains("rate_limit") || msg.contains("rate limit") {
            Self::RateLimit
        } else if msg.contains("529") || msg.contains("overloaded") {
            Self::Overloaded
        } else if msg.contains("timeout")
            || msg.contains("timed out")
            || msg.contains("connection reset")
        {
            Self::Timeout
        } else if msg.contains("500") || msg.contains("502") || msg.contains("503") {
            Self::ServiceError
        } else if msg.contains("402") || msg.contains("credit_balance") || msg.contains("billing") {
            Self::BillingError
        } else if msg.contains("401")
            || msg.contains("403")
            || msg.contains("api_key")
            || msg.contains("unauthorized")
        {
            Self::AuthError
        } else if msg.contains("context_length")
            || msg.contains("context overflow")
            || msg.contains("too long")
        {
            Self::ContextOverflow
        } else {
            Self::Unknown
        }
    }
}

/// 重试策略配置
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数（默认 3）
    pub max_retries: u32,
    /// 基础等待时间（默认 1s）
    pub base_delay: Duration,
    /// 最大等待时间上限（默认 60s）
    pub max_delay: Duration,
    /// 指数退避系数（默认 2.0）
    pub backoff_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
        }
    }
}

impl RetryPolicy {
    /// 计算第 attempt 次重试的等待时间（指数退避）
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let delay_secs = self.base_delay.as_secs_f64() * self.backoff_factor.powi(attempt as i32);
        let clamped = delay_secs.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(clamped)
    }

    /// 判断给定错误和重试次数是否应该继续重试
    pub fn should_retry_str(&self, error_msg: &str, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        LlmErrorKind::classify_from_str(error_msg).is_retryable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rate_limit() {
        assert_eq!(
            LlmErrorKind::classify_from_str("http 429 rate limit exceeded"),
            LlmErrorKind::RateLimit
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("rate_limit error"),
            LlmErrorKind::RateLimit
        );
    }

    #[test]
    fn test_classify_overloaded() {
        assert_eq!(
            LlmErrorKind::classify_from_str("529 overloaded"),
            LlmErrorKind::Overloaded
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("server overloaded"),
            LlmErrorKind::Overloaded
        );
    }

    #[test]
    fn test_classify_timeout() {
        assert_eq!(
            LlmErrorKind::classify_from_str("connection timeout"),
            LlmErrorKind::Timeout
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("request timed out"),
            LlmErrorKind::Timeout
        );
    }

    #[test]
    fn test_classify_service_error() {
        assert_eq!(
            LlmErrorKind::classify_from_str("http 500 internal server error"),
            LlmErrorKind::ServiceError
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("502 bad gateway"),
            LlmErrorKind::ServiceError
        );
    }

    #[test]
    fn test_classify_non_retryable() {
        assert_eq!(
            LlmErrorKind::classify_from_str("401 unauthorized"),
            LlmErrorKind::AuthError
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("402 credit_balance too low"),
            LlmErrorKind::BillingError
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("context_length exceeded"),
            LlmErrorKind::ContextOverflow
        );
        assert_eq!(
            LlmErrorKind::classify_from_str("some unknown error"),
            LlmErrorKind::Unknown
        );
    }

    #[test]
    fn test_is_retryable() {
        assert!(LlmErrorKind::RateLimit.is_retryable());
        assert!(LlmErrorKind::Overloaded.is_retryable());
        assert!(LlmErrorKind::Timeout.is_retryable());
        assert!(LlmErrorKind::ServiceError.is_retryable());
        assert!(!LlmErrorKind::BillingError.is_retryable());
        assert!(!LlmErrorKind::AuthError.is_retryable());
        assert!(!LlmErrorKind::ContextOverflow.is_retryable());
        assert!(!LlmErrorKind::Unknown.is_retryable());
    }

    #[test]
    fn test_retry_policy_delay_exponential() {
        let policy = RetryPolicy::default();
        // attempt 0: 1s * 2^0 = 1s
        assert_eq!(policy.delay_for(0), Duration::from_secs(1));
        // attempt 1: 1s * 2^1 = 2s
        assert_eq!(policy.delay_for(1), Duration::from_secs(2));
        // attempt 2: 1s * 2^2 = 4s
        assert_eq!(policy.delay_for(2), Duration::from_secs(4));
    }

    #[test]
    fn test_retry_policy_max_delay_cap() {
        let policy = RetryPolicy::default();
        // attempt 10: 1s * 2^10 = 1024s, clamped to 60s
        assert_eq!(policy.delay_for(10), Duration::from_secs(60));
    }

    #[test]
    fn test_should_retry_str() {
        let policy = RetryPolicy::default();
        assert!(policy.should_retry_str("429 rate limit", 0));
        assert!(policy.should_retry_str("429 rate limit", 2));
        // attempt >= max_retries (3) => false
        assert!(!policy.should_retry_str("429 rate limit", 3));
        // non-retryable error => false
        assert!(!policy.should_retry_str("401 unauthorized", 0));
    }

    #[test]
    fn test_rate_limit_should_failover() {
        assert!(LlmErrorKind::RateLimit.should_failover());
    }

    #[test]
    fn test_service_error_should_failover() {
        assert!(LlmErrorKind::ServiceError.should_failover());
    }

    #[test]
    fn test_auth_error_should_not_failover() {
        assert!(!LlmErrorKind::AuthError.should_failover());
    }

    #[test]
    fn test_billing_error_should_not_failover() {
        assert!(!LlmErrorKind::BillingError.should_failover());
    }

    #[test]
    fn test_context_overflow_should_not_failover() {
        assert!(!LlmErrorKind::ContextOverflow.should_failover());
    }

    #[test]
    fn test_routing_strategy_retry() {
        assert_eq!(LlmErrorKind::RateLimit.routing_strategy(), ErrorStrategy::Retry);
        assert_eq!(LlmErrorKind::Overloaded.routing_strategy(), ErrorStrategy::Retry);
        assert_eq!(LlmErrorKind::Unknown.routing_strategy(), ErrorStrategy::Retry);
    }

    #[test]
    fn test_routing_strategy_failover() {
        assert_eq!(LlmErrorKind::ServiceError.routing_strategy(), ErrorStrategy::Failover);
        assert_eq!(LlmErrorKind::Timeout.routing_strategy(), ErrorStrategy::Failover);
    }

    #[test]
    fn test_routing_strategy_compact() {
        assert_eq!(
            LlmErrorKind::ContextOverflow.routing_strategy(),
            ErrorStrategy::CompactAndRetry
        );
    }

    #[test]
    fn test_routing_strategy_fail() {
        assert_eq!(LlmErrorKind::AuthError.routing_strategy(), ErrorStrategy::Fail);
        assert_eq!(LlmErrorKind::BillingError.routing_strategy(), ErrorStrategy::Fail);
    }
}
