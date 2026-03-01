// WASM adapter - implementation in Task 3
// Placeholder for WASM sandbox runtime

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};

/// WebAssembly sandbox adapter
/// Will be fully implemented in Task 3
pub struct WasmAdapter;

impl WasmAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WasmAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for WasmAdapter {
    async fn create(&self, _config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        // TODO: Implement in Task 3
        Err(SandboxError::UnsupportedType("Wasm sandbox not yet implemented".into()))
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
        SandboxType::Wasm
    }
}
