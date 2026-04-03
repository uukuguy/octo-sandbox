use std::fmt;
use std::time::Duration;

/// Structured provider error carrying HTTP-level retry information.
///
/// When a provider receives an HTTP error response, it wraps the details into
/// this type so that upstream decorators (RetryProvider, CircuitBreaker, etc.)
/// can make informed retry/failover decisions without parsing error message strings.
#[derive(Debug)]
pub struct ProviderError {
    /// Structured retry information extracted from the HTTP response.
    pub retry_info: RetryInfo,
    /// Human-readable error message (e.g. "Anthropic API error 429: {...}").
    pub message: String,
    /// The HTTP status code.
    pub status: u16,
    /// The response body (may contain JSON error details).
    pub body: String,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    /// Build a ProviderError from raw HTTP response components.
    pub fn from_http_response(
        provider_name: &str,
        status: u16,
        retry_after_header: Option<&str>,
        body: String,
    ) -> Self {
        let retry_info = RetryInfo::from_response(status, retry_after_header, &body);
        let message = format!("{provider_name} API error {status}: {body}");
        Self {
            retry_info,
            message,
            status,
            body,
        }
    }
}

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

/// 从 HTTP 响应中提取的重试信息
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// 错误分类
    pub kind: LlmErrorKind,
    /// 从 Retry-After header 解析的等待时间
    pub retry_after: Option<Duration>,
    /// 从 JSON body 提取的错误代码
    pub error_code: Option<String>,
}

impl RetryInfo {
    /// 从 HTTP status code 和 body 构建
    pub fn from_status_body(status: u16, body: &str) -> Self {
        let kind = Self::classify_status(status, body);
        let error_code = Self::extract_error_code(body);
        Self {
            kind,
            retry_after: None,
            error_code,
        }
    }

    /// 从 HTTP status + Retry-After header + body 构建
    pub fn from_response(status: u16, retry_after_header: Option<&str>, body: &str) -> Self {
        let kind = Self::classify_status(status, body);
        let retry_after = retry_after_header.and_then(Self::parse_retry_after);
        let error_code = Self::extract_error_code(body);
        Self {
            kind,
            retry_after,
            error_code,
        }
    }

    /// 精确分类 HTTP status code
    fn classify_status(status: u16, body: &str) -> LlmErrorKind {
        match status {
            429 => LlmErrorKind::RateLimit,
            402 => LlmErrorKind::BillingError,
            401 | 403 => LlmErrorKind::AuthError,
            529 => LlmErrorKind::Overloaded,
            408 | 504 => LlmErrorKind::Timeout,
            500 | 502 | 503 => LlmErrorKind::ServiceError,
            _ if body.contains("credit_balance_too_low") || body.contains("billing") => {
                LlmErrorKind::BillingError
            }
            _ if body.contains("context_length_exceeded") || body.contains("too long") => {
                LlmErrorKind::ContextOverflow
            }
            _ => LlmErrorKind::Unknown,
        }
    }

    /// 解析 Retry-After header（秒数或 HTTP-date）
    fn parse_retry_after(value: &str) -> Option<Duration> {
        // 尝试解析为秒数
        if let Ok(secs) = value.trim().parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
        // 大多数 API 返回秒数格式；HTTP-date 格式暂不支持
        None
    }

    /// 从 JSON body 提取错误代码
    fn extract_error_code(body: &str) -> Option<String> {
        // 尝试解析 {"error": {"type": "..."}} 或 {"error": {"code": "..."}}
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(err) = v.get("error") {
                if let Some(code) = err.get("type").or_else(|| err.get("code")) {
                    return code.as_str().map(|s| s.to_string());
                }
            }
        }
        None
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
    /// When true, retries indefinitely for transient errors (RateLimit, Overloaded, etc.).
    /// AuthError and other non-retryable errors still fail immediately.
    pub unattended: bool,
    /// Max delay between retries in unattended mode (default 5 min).
    pub unattended_max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
            unattended: false,
            unattended_max_delay: Duration::from_secs(300),
        }
    }
}

impl RetryPolicy {
    /// Create an unattended retry policy for autonomous agents.
    /// Retries indefinitely for transient errors with 5-min max backoff.
    pub fn unattended() -> Self {
        Self {
            max_retries: u32::MAX,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
            unattended: true,
            unattended_max_delay: Duration::from_secs(300),
        }
    }

    /// 计算第 attempt 次重试的等待时间（指数退避）
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let delay_secs = self.base_delay.as_secs_f64() * self.backoff_factor.powi(attempt as i32);
        let clamped = delay_secs.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(clamped)
    }

    /// 使用 RetryInfo 判断是否重试，并计算延迟
    /// 如果有 Retry-After header，优先使用其值
    pub fn should_retry_with_info(&self, info: &RetryInfo, attempt: u32) -> Option<Duration> {
        // Non-retryable errors always fail immediately
        if !info.kind.is_retryable() {
            return None;
        }
        // Check attempt limit (unattended mode ignores this)
        if !self.unattended && attempt >= self.max_retries {
            return None;
        }
        // 优先使用 Retry-After，否则用指数退避
        let delay = info.retry_after.unwrap_or_else(|| self.delay_for(attempt));
        let cap = if self.unattended { self.unattended_max_delay } else { self.max_delay };
        Some(delay.min(cap))
    }

    /// 判断给定错误和重试次数是否应该继续重试
    pub fn should_retry_str(&self, error_msg: &str, attempt: u32) -> bool {
        if !self.unattended && attempt >= self.max_retries {
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
        assert_eq!(
            LlmErrorKind::RateLimit.routing_strategy(),
            ErrorStrategy::Retry
        );
        assert_eq!(
            LlmErrorKind::Overloaded.routing_strategy(),
            ErrorStrategy::Retry
        );
        assert_eq!(
            LlmErrorKind::Unknown.routing_strategy(),
            ErrorStrategy::Retry
        );
    }

    #[test]
    fn test_routing_strategy_failover() {
        assert_eq!(
            LlmErrorKind::ServiceError.routing_strategy(),
            ErrorStrategy::Failover
        );
        assert_eq!(
            LlmErrorKind::Timeout.routing_strategy(),
            ErrorStrategy::Failover
        );
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
        assert_eq!(
            LlmErrorKind::AuthError.routing_strategy(),
            ErrorStrategy::Fail
        );
        assert_eq!(
            LlmErrorKind::BillingError.routing_strategy(),
            ErrorStrategy::Fail
        );
    }

    // === RetryInfo tests ===

    #[test]
    fn test_retry_info_rate_limit() {
        let info = RetryInfo::from_status_body(429, "rate limit exceeded");
        assert_eq!(info.kind, LlmErrorKind::RateLimit);
    }

    #[test]
    fn test_retry_info_billing() {
        let info = RetryInfo::from_status_body(402, "insufficient credits");
        assert_eq!(info.kind, LlmErrorKind::BillingError);
    }

    #[test]
    fn test_retry_info_auth() {
        let info = RetryInfo::from_status_body(401, "unauthorized");
        assert_eq!(info.kind, LlmErrorKind::AuthError);
        let info403 = RetryInfo::from_status_body(403, "forbidden");
        assert_eq!(info403.kind, LlmErrorKind::AuthError);
    }

    #[test]
    fn test_retry_info_overloaded() {
        let info = RetryInfo::from_status_body(529, "overloaded");
        assert_eq!(info.kind, LlmErrorKind::Overloaded);
    }

    #[test]
    fn test_retry_info_timeout() {
        let info = RetryInfo::from_status_body(408, "request timeout");
        assert_eq!(info.kind, LlmErrorKind::Timeout);
        let info504 = RetryInfo::from_status_body(504, "gateway timeout");
        assert_eq!(info504.kind, LlmErrorKind::Timeout);
    }

    #[test]
    fn test_retry_info_service_error() {
        for status in [500, 502, 503] {
            let info = RetryInfo::from_status_body(status, "server error");
            assert_eq!(info.kind, LlmErrorKind::ServiceError);
        }
    }

    #[test]
    fn test_retry_info_body_billing_fallback() {
        let info = RetryInfo::from_status_body(200, "credit_balance_too_low");
        assert_eq!(info.kind, LlmErrorKind::BillingError);
        let info2 = RetryInfo::from_status_body(200, "billing issue");
        assert_eq!(info2.kind, LlmErrorKind::BillingError);
    }

    #[test]
    fn test_retry_info_body_context_overflow_fallback() {
        let info = RetryInfo::from_status_body(200, "context_length_exceeded");
        assert_eq!(info.kind, LlmErrorKind::ContextOverflow);
        let info2 = RetryInfo::from_status_body(200, "input too long");
        assert_eq!(info2.kind, LlmErrorKind::ContextOverflow);
    }

    #[test]
    fn test_retry_info_with_retry_after_seconds() {
        let info = RetryInfo::from_response(429, Some("30"), "rate limited");
        assert_eq!(info.retry_after, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_retry_info_with_retry_after_none() {
        let info = RetryInfo::from_response(500, None, "server error");
        assert!(info.retry_after.is_none());
    }

    #[test]
    fn test_retry_info_with_retry_after_invalid() {
        let info = RetryInfo::from_response(429, Some("not-a-number"), "rate limited");
        assert!(info.retry_after.is_none());
    }

    #[test]
    fn test_retry_info_extract_error_code() {
        let body =
            r#"{"error": {"type": "rate_limit_error", "message": "too many requests"}}"#;
        let info = RetryInfo::from_status_body(429, body);
        assert_eq!(info.error_code.as_deref(), Some("rate_limit_error"));
    }

    #[test]
    fn test_retry_info_extract_error_code_from_code_field() {
        let body = r#"{"error": {"code": "insufficient_quota", "message": "out of quota"}}"#;
        let info = RetryInfo::from_status_body(402, body);
        assert_eq!(info.error_code.as_deref(), Some("insufficient_quota"));
    }

    #[test]
    fn test_retry_info_no_error_code_in_plain_text() {
        let info = RetryInfo::from_status_body(500, "internal server error");
        assert!(info.error_code.is_none());
    }

    #[test]
    fn test_should_retry_with_info_respects_retry_after() {
        let policy = RetryPolicy::default();
        let info = RetryInfo {
            kind: LlmErrorKind::RateLimit,
            retry_after: Some(Duration::from_secs(15)),
            error_code: None,
        };
        let delay = policy.should_retry_with_info(&info, 0);
        assert_eq!(delay, Some(Duration::from_secs(15)));
    }

    #[test]
    fn test_should_retry_with_info_falls_back_to_backoff() {
        let policy = RetryPolicy::default();
        let info = RetryInfo {
            kind: LlmErrorKind::RateLimit,
            retry_after: None,
            error_code: None,
        };
        // attempt 0 → base_delay * 2^0 = 1s
        let delay = policy.should_retry_with_info(&info, 0);
        assert_eq!(delay, Some(Duration::from_secs(1)));
    }

    #[test]
    fn test_should_retry_with_info_non_retryable() {
        let policy = RetryPolicy::default();
        let info = RetryInfo::from_status_body(401, "unauthorized");
        assert!(policy.should_retry_with_info(&info, 0).is_none());
    }

    #[test]
    fn test_should_retry_with_info_max_retries_exceeded() {
        let policy = RetryPolicy::default();
        let info = RetryInfo::from_status_body(429, "rate limited");
        assert!(policy.should_retry_with_info(&info, 3).is_none());
    }

    #[test]
    fn test_should_retry_with_info_caps_at_max_delay() {
        let policy = RetryPolicy::default(); // max_delay = 60s
        let info = RetryInfo {
            kind: LlmErrorKind::RateLimit,
            retry_after: Some(Duration::from_secs(120)),
            error_code: None,
        };
        let delay = policy.should_retry_with_info(&info, 0);
        assert_eq!(delay, Some(Duration::from_secs(60)));
    }
}
