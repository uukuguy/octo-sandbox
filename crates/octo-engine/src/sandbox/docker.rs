// Docker adapter - implementation in Task 5
// Placeholder for Docker sandbox runtime

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};

/// Docker container sandbox adapter
/// Will be fully implemented in Task 5
pub struct DockerAdapter;

impl DockerAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DockerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for DockerAdapter {
    async fn create(&self, _config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        // TODO: Implement in Task 5
        Err(SandboxError::UnsupportedType("Docker sandbox not yet implemented".into()))
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        // TODO: Implement in Task 5
        Err(SandboxError::NotFound(id.clone()))
    }

    async fn execute(
        &self,
        id: &SandboxId,
        _code: &str,
        _language: &str,
    ) -> Result<ExecResult, SandboxError> {
        // TODO: Implement in Task 5
        Err(SandboxError::NotFound(id.clone()))
    }

    fn sandbox_type(&self) -> SandboxType {
        SandboxType::Docker
    }
}
