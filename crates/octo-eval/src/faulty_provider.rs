//! FaultyProvider — wraps any Provider and injects configurable errors at specific turns.
//!
//! Used by the resilience suite to test agent retry/recovery behavior.
//! On the configured fail_turn (1-based), returns an error instead of the real response.
//! All other turns pass through to the inner provider normally.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use octo_engine::providers::{CompletionStream, Provider};
use octo_engine::Metering;
use octo_types::{CompletionRequest, CompletionResponse};

/// Error type to inject
#[derive(Debug, Clone)]
pub enum FaultType {
    /// HTTP 429 rate limit exceeded
    RateLimit,
    /// HTTP 504 gateway timeout
    Timeout,
    /// HTTP 500 internal server error
    ServerError,
    /// Network connection refused
    ConnectionRefused,
}

impl FaultType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "rate_limit" | "429" => FaultType::RateLimit,
            "timeout" | "504" => FaultType::Timeout,
            "server_error" | "500" => FaultType::ServerError,
            "connection_refused" => FaultType::ConnectionRefused,
            _ => FaultType::ServerError,
        }
    }

    fn error_message(&self) -> &'static str {
        match self {
            FaultType::RateLimit => "Rate limit exceeded (429): Too Many Requests",
            FaultType::Timeout => "Gateway timeout (504): Request timed out after 30s",
            FaultType::ServerError => "Internal server error (500): Service temporarily unavailable",
            FaultType::ConnectionRefused => "Connection refused: endpoint unreachable",
        }
    }
}

/// A provider wrapper that injects a fault at a specific turn, then passes through normally.
pub struct FaultyProvider {
    inner: Arc<dyn Provider>,
    /// 1-based turn number at which to inject the fault (e.g., 1 = first call fails)
    fail_turn: u32,
    fault_type: FaultType,
    call_count: AtomicU32,
}

impl FaultyProvider {
    pub fn new(inner: Arc<dyn Provider>, fail_turn: u32, fault_type: FaultType) -> Self {
        Self {
            inner,
            fail_turn,
            fault_type,
            call_count: AtomicU32::new(0),
        }
    }

    pub fn from_config(inner: Arc<dyn Provider>, fail_turn: u32, error_type: &str) -> Self {
        Self::new(inner, fail_turn, FaultType::from_str(error_type))
    }
}

#[async_trait]
impl Provider for FaultyProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn metering(&self) -> Option<Arc<Metering>> {
        self.inner.metering()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let turn = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if turn == self.fail_turn {
            return Err(anyhow::anyhow!("{}", self.fault_type.error_message()));
        }
        self.inner.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        let turn = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if turn == self.fail_turn {
            return Err(anyhow::anyhow!("{}", self.fault_type.error_message()));
        }
        self.inner.stream(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fault_type_from_str() {
        assert!(matches!(FaultType::from_str("rate_limit"), FaultType::RateLimit));
        assert!(matches!(FaultType::from_str("429"), FaultType::RateLimit));
        assert!(matches!(FaultType::from_str("timeout"), FaultType::Timeout));
        assert!(matches!(FaultType::from_str("server_error"), FaultType::ServerError));
        assert!(matches!(FaultType::from_str("unknown"), FaultType::ServerError));
    }

    #[test]
    fn test_fault_type_messages() {
        assert!(FaultType::RateLimit.error_message().contains("429"));
        assert!(FaultType::Timeout.error_message().contains("504"));
        assert!(FaultType::ServerError.error_message().contains("500"));
    }
}
