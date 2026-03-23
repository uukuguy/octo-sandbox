//! Sandbox audit event types
//!
//! Provides structured audit events for sandbox operations.
//! Events are converted to generic AuditEvent for storage via the existing
//! AuditStorage hash-chain infrastructure.

use super::traits::{SandboxPolicy, SandboxType};
use crate::audit::AuditEvent;
use sha2::{Digest, Sha256};

/// Action performed in the sandbox
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxAction {
    Create,
    Execute,
    Destroy,
    PolicyDeny,
    DegradationWarning,
    ResourceExceeded,
    Timeout,
}

impl std::fmt::Display for SandboxAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxAction::Create => write!(f, "Create"),
            SandboxAction::Execute => write!(f, "Execute"),
            SandboxAction::Destroy => write!(f, "Destroy"),
            SandboxAction::PolicyDeny => write!(f, "PolicyDeny"),
            SandboxAction::DegradationWarning => write!(f, "DegradationWarning"),
            SandboxAction::ResourceExceeded => write!(f, "ResourceExceeded"),
            SandboxAction::Timeout => write!(f, "Timeout"),
        }
    }
}

/// Resource usage statistics for a sandbox execution
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub memory_peak_bytes: Option<u64>,
    pub cpu_time_ms: Option<u64>,
}

/// Structured sandbox audit event
///
/// Captures detailed information about sandbox operations for compliance
/// and debugging. Converts to generic `AuditEvent` for hash-chain storage.
#[derive(Debug, Clone)]
pub struct SandboxAuditEvent {
    pub sandbox_type: SandboxType,
    pub sandbox_id: String,
    pub action: SandboxAction,
    pub language: String,
    pub code_hash: String,
    pub image: Option<String>,
    pub exit_code: Option<i32>,
    pub execution_time_ms: u64,
    pub stdout_size: usize,
    pub stderr_size: usize,
    pub policy: SandboxPolicy,
    pub was_degraded: bool,
    pub resource_usage: Option<ResourceUsage>,
}

impl SandboxAuditEvent {
    /// Create an execution audit event
    pub fn execution(
        sandbox_type: SandboxType,
        sandbox_id: &str,
        code: &str,
        language: &str,
        exit_code: i32,
        execution_time_ms: u64,
        stdout_size: usize,
        stderr_size: usize,
        policy: SandboxPolicy,
        was_degraded: bool,
    ) -> Self {
        Self {
            sandbox_type,
            sandbox_id: sandbox_id.to_string(),
            action: SandboxAction::Execute,
            language: language.to_string(),
            code_hash: Self::hash_code(code),
            image: None,
            exit_code: Some(exit_code),
            execution_time_ms,
            stdout_size,
            stderr_size,
            policy,
            was_degraded,
            resource_usage: None,
        }
    }

    /// Create a policy denial audit event
    pub fn policy_deny(
        sandbox_type: SandboxType,
        code: &str,
        language: &str,
        policy: SandboxPolicy,
    ) -> Self {
        Self {
            sandbox_type,
            sandbox_id: String::new(),
            action: SandboxAction::PolicyDeny,
            language: language.to_string(),
            code_hash: Self::hash_code(code),
            image: None,
            exit_code: None,
            execution_time_ms: 0,
            stdout_size: 0,
            stderr_size: 0,
            policy,
            was_degraded: false,
            resource_usage: None,
        }
    }

    /// Create a degradation warning audit event
    pub fn degradation(
        target_type: SandboxType,
        actual_type: SandboxType,
        code: &str,
        language: &str,
        policy: SandboxPolicy,
    ) -> Self {
        let image = format!("{} -> {}", target_type, actual_type);
        Self {
            sandbox_type: actual_type,
            sandbox_id: String::new(),
            action: SandboxAction::DegradationWarning,
            language: language.to_string(),
            code_hash: Self::hash_code(code),
            image: Some(image),
            exit_code: None,
            execution_time_ms: 0,
            stdout_size: 0,
            stderr_size: 0,
            policy,
            was_degraded: true,
            resource_usage: None,
        }
    }

    /// Convert to generic AuditEvent for hash-chain storage
    pub fn to_audit_event(&self, session_id: Option<&str>) -> AuditEvent {
        let metadata = serde_json::json!({
            "sandbox_type": self.sandbox_type.to_string(),
            "sandbox_id": self.sandbox_id,
            "language": self.language,
            "code_hash": self.code_hash,
            "image": self.image,
            "exit_code": self.exit_code,
            "execution_time_ms": self.execution_time_ms,
            "stdout_size": self.stdout_size,
            "stderr_size": self.stderr_size,
            "policy": self.policy.to_string(),
            "was_degraded": self.was_degraded,
        });

        AuditEvent {
            event_type: "sandbox".to_string(),
            user_id: None,
            session_id: session_id.map(|s| s.to_string()),
            resource_id: Some(self.sandbox_id.clone()),
            action: self.action.to_string(),
            result: if self.exit_code == Some(0) {
                "success"
            } else {
                "failure"
            }
            .to_string(),
            metadata: Some(metadata),
            ip_address: None,
        }
    }

    /// Compute SHA-256 hash of code for audit trail
    fn hash_code(code: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(code.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_event() {
        let event = SandboxAuditEvent::execution(
            SandboxType::Docker,
            "sandbox-123",
            "echo hello",
            "bash",
            0,
            150,
            6,
            0,
            SandboxPolicy::Strict,
            false,
        );

        assert_eq!(event.action, SandboxAction::Execute);
        assert_eq!(event.sandbox_id, "sandbox-123");
        assert_eq!(event.exit_code, Some(0));
        assert!(!event.code_hash.is_empty());
    }

    #[test]
    fn test_policy_deny_event() {
        let event = SandboxAuditEvent::policy_deny(
            SandboxType::Subprocess,
            "rm -rf /",
            "bash",
            SandboxPolicy::Strict,
        );

        assert_eq!(event.action, SandboxAction::PolicyDeny);
        assert_eq!(event.policy, SandboxPolicy::Strict);
    }

    #[test]
    fn test_degradation_event() {
        let event = SandboxAuditEvent::degradation(
            SandboxType::Docker,
            SandboxType::Subprocess,
            "echo test",
            "bash",
            SandboxPolicy::Preferred,
        );

        assert_eq!(event.action, SandboxAction::DegradationWarning);
        assert!(event.was_degraded);
        assert_eq!(event.image, Some("docker -> subprocess".to_string()));
    }

    #[test]
    fn test_to_audit_event() {
        let event = SandboxAuditEvent::execution(
            SandboxType::Docker,
            "sandbox-456",
            "python script.py",
            "python",
            0,
            1200,
            100,
            0,
            SandboxPolicy::Strict,
            false,
        );

        let audit = event.to_audit_event(Some("session-1"));
        assert_eq!(audit.event_type, "sandbox");
        assert_eq!(audit.action, "Execute");
        assert_eq!(audit.result, "success");
        assert_eq!(audit.session_id, Some("session-1".to_string()));
        assert!(audit.metadata.is_some());

        let meta = audit.metadata.unwrap();
        assert_eq!(meta["sandbox_type"], "docker");
        assert_eq!(meta["language"], "python");
    }

    #[test]
    fn test_code_hash_deterministic() {
        let h1 = SandboxAuditEvent::hash_code("echo hello");
        let h2 = SandboxAuditEvent::hash_code("echo hello");
        let h3 = SandboxAuditEvent::hash_code("echo world");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_failed_execution_event() {
        let event = SandboxAuditEvent::execution(
            SandboxType::Subprocess,
            "sandbox-789",
            "false",
            "bash",
            1,
            50,
            0,
            20,
            SandboxPolicy::Development,
            false,
        );

        let audit = event.to_audit_event(None);
        assert_eq!(audit.result, "failure");
        assert!(audit.session_id.is_none());
    }
}
