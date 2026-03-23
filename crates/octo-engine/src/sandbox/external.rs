//! External sandbox provider trait
//!
//! Defines the interface for third-party sandbox services (E2B, Modal,
//! Firecracker, gVisor, etc.) that provide remote or specialized isolation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use super::traits::{ExecResult, SandboxError};

/// Unique identifier for an external sandbox instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalSandboxId(String);

impl ExternalSandboxId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExternalSandboxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Configuration for creating an external sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSandboxConfig {
    /// Provider-specific image or template name
    #[serde(default)]
    pub image: Option<String>,
    /// Environment variables to inject
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Memory limit in MB
    #[serde(default)]
    pub memory_mb: Option<u64>,
    /// CPU cores
    #[serde(default)]
    pub cpu_cores: Option<u32>,
    /// Timeout for sandbox lifecycle in seconds
    #[serde(default = "default_lifecycle_timeout")]
    pub lifecycle_timeout_secs: u64,
    /// Provider-specific metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_lifecycle_timeout() -> u64 {
    300
}

impl Default for ExternalSandboxConfig {
    fn default() -> Self {
        Self {
            image: None,
            env: HashMap::new(),
            memory_mb: None,
            cpu_cores: None,
            lifecycle_timeout_secs: default_lifecycle_timeout(),
            metadata: HashMap::new(),
        }
    }
}

/// Request to execute code in an external sandbox.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    /// Command to execute
    pub command: String,
    /// Working directory inside the sandbox
    pub working_dir: Option<String>,
    /// Standard input data
    pub stdin: Option<String>,
    /// Execution timeout in seconds
    pub timeout_secs: u64,
    /// Language hint for the executor
    pub language: String,
}

impl ExecRequest {
    pub fn new(command: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            working_dir: None,
            stdin: None,
            timeout_secs: 30,
            language: language.into(),
        }
    }

    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Trait for external sandbox providers.
///
/// Implementors provide lifecycle management and code execution
/// for third-party sandbox services.
///
/// # Lifecycle
/// ```text
/// create() → execute() [repeat] → destroy()
/// ```
#[async_trait::async_trait]
pub trait ExternalSandboxProvider: Send + Sync {
    /// Provider name (e.g., "e2b", "modal", "firecracker")
    fn name(&self) -> &str;

    /// Create a new sandbox instance.
    async fn create(
        &self,
        config: &ExternalSandboxConfig,
    ) -> Result<ExternalSandboxId, SandboxError>;

    /// Execute code in the sandbox.
    async fn execute(
        &self,
        id: &ExternalSandboxId,
        request: &ExecRequest,
    ) -> Result<ExecResult, SandboxError>;

    /// Upload a file to the sandbox.
    async fn upload(
        &self,
        id: &ExternalSandboxId,
        remote_path: &str,
        content: &[u8],
    ) -> Result<(), SandboxError>;

    /// Download a file from the sandbox.
    async fn download(
        &self,
        id: &ExternalSandboxId,
        remote_path: &str,
    ) -> Result<Vec<u8>, SandboxError>;

    /// Destroy the sandbox instance.
    async fn destroy(&self, id: &ExternalSandboxId) -> Result<(), SandboxError>;

    /// Check if the provider is healthy and available.
    async fn health_check(&self) -> Result<bool, SandboxError>;
}

/// Stub E2B provider — demonstrates the trait interface.
/// Does not implement actual API calls (deferred to AB-D2).
pub struct StubE2BProvider;

impl StubE2BProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubE2BProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ExternalSandboxProvider for StubE2BProvider {
    fn name(&self) -> &str {
        "e2b-stub"
    }

    async fn create(
        &self,
        _config: &ExternalSandboxConfig,
    ) -> Result<ExternalSandboxId, SandboxError> {
        Err(SandboxError::UnsupportedType(
            "E2B provider is a stub — full implementation deferred to AB-D2".to_string(),
        ))
    }

    async fn execute(
        &self,
        _id: &ExternalSandboxId,
        _request: &ExecRequest,
    ) -> Result<ExecResult, SandboxError> {
        Err(SandboxError::UnsupportedType(
            "E2B provider is a stub".to_string(),
        ))
    }

    async fn upload(
        &self,
        _id: &ExternalSandboxId,
        _remote_path: &str,
        _content: &[u8],
    ) -> Result<(), SandboxError> {
        Err(SandboxError::UnsupportedType(
            "E2B provider is a stub".to_string(),
        ))
    }

    async fn download(
        &self,
        _id: &ExternalSandboxId,
        _remote_path: &str,
    ) -> Result<Vec<u8>, SandboxError> {
        Err(SandboxError::UnsupportedType(
            "E2B provider is a stub".to_string(),
        ))
    }

    async fn destroy(&self, _id: &ExternalSandboxId) -> Result<(), SandboxError> {
        Ok(()) // Destroy is always safe to call
    }

    async fn health_check(&self) -> Result<bool, SandboxError> {
        Ok(false) // Stub is never "healthy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_sandbox_id() {
        let id = ExternalSandboxId::new("sandbox-abc");
        assert_eq!(id.as_str(), "sandbox-abc");
        assert_eq!(id.to_string(), "sandbox-abc");
    }

    #[test]
    fn test_external_sandbox_config_default() {
        let config = ExternalSandboxConfig::default();
        assert!(config.image.is_none());
        assert!(config.env.is_empty());
        assert_eq!(config.lifecycle_timeout_secs, 300);
    }

    #[test]
    fn test_exec_request() {
        let req = ExecRequest::new("echo hello", "bash")
            .with_working_dir("/workspace")
            .with_timeout(60);

        assert_eq!(req.command, "echo hello");
        assert_eq!(req.language, "bash");
        assert_eq!(req.working_dir, Some("/workspace".to_string()));
        assert_eq!(req.timeout_secs, 60);
    }

    #[tokio::test]
    async fn test_stub_e2b_provider_name() {
        let provider = StubE2BProvider::new();
        assert_eq!(provider.name(), "e2b-stub");
    }

    #[tokio::test]
    async fn test_stub_e2b_provider_create_fails() {
        let provider = StubE2BProvider::new();
        let config = ExternalSandboxConfig::default();
        let result = provider.create(&config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stub_e2b_provider_health_check() {
        let provider = StubE2BProvider::new();
        let result = provider.health_check().await;
        assert!(matches!(result, Ok(false)));
    }

    #[tokio::test]
    async fn test_stub_e2b_provider_destroy_ok() {
        let provider = StubE2BProvider::new();
        let id = ExternalSandboxId::new("test");
        let result = provider.destroy(&id).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let mut config = ExternalSandboxConfig::default();
        config.image = Some("python:3.12".to_string());
        config.memory_mb = Some(512);
        config.env.insert("KEY".to_string(), "value".to_string());

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ExternalSandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.image, Some("python:3.12".to_string()));
        assert_eq!(deserialized.memory_mb, Some(512));
        assert_eq!(deserialized.env.get("KEY"), Some(&"value".to_string()));
    }
}
