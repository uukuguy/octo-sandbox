// Subprocess adapter - implementation in Task 3
// Placeholder for subprocess sandbox runtime

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};

/// Local subprocess sandbox adapter
/// Will be fully implemented in Task 3
pub struct SubprocessAdapter;

impl SubprocessAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubprocessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for SubprocessAdapter {
    async fn create(&self, _config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        // TODO: Implement in Task 3
        Err(SandboxError::UnsupportedType("Subprocess sandbox not yet implemented".into()))
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        // TODO: Implement in Task 3
        Err(SandboxError::NotFound(id.clone()))
    }

    async fn execute(
        &self,
        id: &SandboxId,
        _code: &str,
        _language: &str,
    ) -> Result<ExecResult, SandboxError> {
        // TODO: Implement in Task 3
        Err(SandboxError::NotFound(id.clone()))
    }

    fn sandbox_type(&self) -> SandboxType {
        SandboxType::Subprocess
    }
}
