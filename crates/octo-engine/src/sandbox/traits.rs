// Placeholder - full implementation in Task 2
// Will include: RuntimeAdapter trait, SandboxType, SandboxConfig, ExecResult, SandboxId, SandboxError

use std::fmt;

/// Unique identifier for a sandbox instance
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SandboxId(String);

impl SandboxId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for SandboxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type of sandbox runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxType {
    /// WebAssembly runtime
    Wasm,
    /// Docker container
    Docker,
    /// Local subprocess
    Subprocess,
}

impl fmt::Display for SandboxType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxType::Wasm => write!(f, "wasm"),
            SandboxType::Docker => write!(f, "docker"),
            SandboxType::Subprocess => write!(f, "subprocess"),
        }
    }
}

/// Configuration for sandbox creation
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Sandbox type
    pub sandbox_type: SandboxType,
    /// Working directory
    pub working_dir: Option<std::path::PathBuf>,
    /// Environment variables
    pub env: std::collections::HashMap<String, String>,
    /// Memory limit in bytes
    pub memory_limit: Option<u64>,
    /// CPU time limit in seconds
    pub time_limit: Option<u64>,
}

impl SandboxConfig {
    pub fn new(sandbox_type: SandboxType) -> Self {
        Self {
            sandbox_type,
            working_dir: None,
            env: std::collections::HashMap::new(),
            memory_limit: None,
            time_limit: None,
        }
    }

    pub fn with_working_dir(mut self, path: std::path::PathBuf) -> Self {
        self.working_dir = Some(path);
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.memory_limit = Some(limit);
        self
    }

    pub fn with_time_limit(mut self, limit: u64) -> Self {
        self.time_limit = Some(limit);
        self
    }
}

/// Result of code execution
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code
    pub exit_code: i32,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether execution was successful
    pub success: bool,
}

/// Errors that can occur during sandbox operations
#[derive(Debug)]
pub enum SandboxError {
    /// Sandbox not found
    NotFound(SandboxId),
    /// Sandbox already exists
    AlreadyExists(SandboxId),
    /// Execution failed
    ExecutionFailed(String),
    /// Configuration error
    ConfigError(String),
    /// IO error
    IoError(std::io::Error),
    /// Serialization error
    SerdeError(serde_json::Error),
    /// Unsupported sandbox type
    UnsupportedType(String),
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::NotFound(id) => write!(f, "Sandbox not found: {}", id),
            SandboxError::AlreadyExists(id) => write!(f, "Sandbox already exists: {}", id),
            SandboxError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            SandboxError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            SandboxError::IoError(e) => write!(f, "IO error: {}", e),
            SandboxError::SerdeError(e) => write!(f, "Serialization error: {}", e),
            SandboxError::UnsupportedType(msg) => write!(f, "Unsupported sandbox type: {}", msg),
        }
    }
}

impl std::error::Error for SandboxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SandboxError::IoError(e) => Some(e),
            SandboxError::SerdeError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        SandboxError::IoError(e)
    }
}

impl From<serde_json::Error> for SandboxError {
    fn from(e: serde_json::Error) -> Self {
        SandboxError::SerdeError(e)
    }
}

/// Runtime adapter trait for sandbox execution
/// Will be fully implemented in Task 2
pub trait RuntimeAdapter: Send + Sync {
    /// Create a new sandbox instance
    fn create(&self, config: &SandboxConfig) -> impl std::future::Future<Output = Result<SandboxId, SandboxError>> + Send;

    /// Destroy a sandbox instance
    fn destroy(&self, id: &SandboxId) -> impl std::future::Future<Output = Result<(), SandboxError>> + Send;

    /// Execute code in a sandbox
    fn execute(
        &self,
        id: &SandboxId,
        code: &str,
        language: &str,
    ) -> impl std::future::Future<Output = Result<ExecResult, SandboxError>> + Send;

    /// Get sandbox type
    fn sandbox_type(&self) -> SandboxType;

    /// Check if sandbox is ready
    fn is_ready(&self) -> impl std::future::Future<Output = bool> + Send {
        async { true }
    }
}
