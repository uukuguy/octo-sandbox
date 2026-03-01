//! Sandbox router for routing tools to appropriate sandbox types
//!
//! This module provides a router that maps tool categories to sandbox types
//! and routes execution requests to the appropriate adapter.

use super::{ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType};
use std::collections::HashMap;
use std::sync::Arc;

/// Tool category to sandbox mapping
///
/// Each category represents a different type of tool execution
/// with different sandbox requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// Shell commands that need full shell access
    Shell,
    /// Stateless computation tasks
    Compute,
    /// File system operations
    FileSystem,
    /// Network requests
    Network,
}

/// Enum wrapper for runtime adapters
///
/// This enum wraps the different adapter types to provide a unified interface
/// since the RuntimeAdapter trait is not dyn-compatible.
pub enum AdapterEnum {
    /// Subprocess adapter
    Subprocess(crate::sandbox::SubprocessAdapter),
    /// WASM adapter
    Wasm(crate::sandbox::WasmAdapter),
    /// Docker adapter
    Docker(crate::sandbox::DockerAdapter),
}

impl AdapterEnum {
    /// Get the sandbox type for this adapter
    pub fn sandbox_type(&self) -> SandboxType {
        match self {
            AdapterEnum::Subprocess(_) => SandboxType::Subprocess,
            AdapterEnum::Wasm(_) => SandboxType::Wasm,
            AdapterEnum::Docker(_) => SandboxType::Docker,
        }
    }

    /// Create a new sandbox instance
    pub async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, SandboxError> {
        match self {
            AdapterEnum::Subprocess(adapter) => adapter.create(config).await,
            AdapterEnum::Wasm(adapter) => adapter.create(config).await,
            AdapterEnum::Docker(adapter) => adapter.create(config).await,
        }
    }

    /// Destroy a sandbox instance
    pub async fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        match self {
            AdapterEnum::Subprocess(adapter) => adapter.destroy(id).await,
            AdapterEnum::Wasm(adapter) => adapter.destroy(id).await,
            AdapterEnum::Docker(adapter) => adapter.destroy(id).await,
        }
    }

    /// Execute code in a sandbox
    pub async fn execute(
        &self,
        id: &SandboxId,
        code: &str,
        language: &str,
    ) -> Result<ExecResult, SandboxError> {
        match self {
            AdapterEnum::Subprocess(adapter) => adapter.execute(id, code, language).await,
            AdapterEnum::Wasm(adapter) => adapter.execute(id, code, language).await,
            AdapterEnum::Docker(adapter) => adapter.execute(id, code, language).await,
        }
    }
}

/// Sandbox router for managing tool routing to sandbox types
///
/// Routes tool execution requests to the appropriate sandbox adapter
/// based on the tool category.
pub struct SandboxRouter {
    /// Registered adapters by sandbox type
    adapters: HashMap<SandboxType, Arc<AdapterEnum>>,
    /// Default sandbox type when no mapping found
    default_sandbox: SandboxType,
    /// Tool category to sandbox type mapping
    tool_mapping: HashMap<ToolCategory, SandboxType>,
}

impl SandboxRouter {
    /// Create a new SandboxRouter with default mappings
    ///
    /// Default mappings:
    /// - Shell -> Docker
    /// - Compute -> Wasm
    /// - FileSystem -> Docker
    /// - Network -> Wasm
    pub fn new() -> Self {
        let mut tool_mapping = HashMap::new();
        tool_mapping.insert(ToolCategory::Shell, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Compute, SandboxType::Wasm);
        tool_mapping.insert(ToolCategory::FileSystem, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Network, SandboxType::Wasm);

        Self {
            adapters: HashMap::new(),
            default_sandbox: SandboxType::Subprocess,
            tool_mapping,
        }
    }

    /// Register an adapter for a specific sandbox type
    pub fn register_adapter(&mut self, adapter: AdapterEnum) {
        let sandbox_type = adapter.sandbox_type();
        self.adapters.insert(sandbox_type, Arc::new(adapter));
    }

    /// Set the default sandbox type
    pub fn set_default(&mut self, sandbox_type: SandboxType) {
        self.default_sandbox = sandbox_type;
    }

    /// Get the sandbox type for a tool category
    pub fn get_sandbox_type(&self, category: ToolCategory) -> SandboxType {
        self.tool_mapping
            .get(&category)
            .copied()
            .unwrap_or(self.default_sandbox)
    }

    /// Get an adapter by sandbox type
    pub fn get_adapter(&self, sandbox_type: SandboxType) -> Option<&Arc<AdapterEnum>> {
        self.adapters.get(&sandbox_type)
    }

    /// Execute a command in the appropriate sandbox
    ///
    /// Creates a temporary sandbox, executes the command, and destroys
    /// the sandbox after execution.
    pub async fn execute(
        &self,
        category: ToolCategory,
        code: &str,
        language: &str,
    ) -> Result<ExecResult, SandboxError> {
        let sandbox_type = self.get_sandbox_type(category);
        let adapter = self
            .adapters
            .get(&sandbox_type)
            .ok_or_else(|| SandboxError::UnsupportedType(format!("{:?} adapter not registered", sandbox_type)))?;

        let config = SandboxConfig::new(sandbox_type);
        let id = adapter.create(&config).await?;
        let result = adapter.execute(&id, code, language).await;
        let _ = adapter.destroy(&id).await;

        result
    }

    /// Override the sandbox type for a specific tool category
    pub fn set_mapping(&mut self, category: ToolCategory, sandbox_type: SandboxType) {
        self.tool_mapping.insert(category, sandbox_type);
    }
}

impl Default for SandboxRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::{DockerAdapter, SubprocessAdapter, WasmAdapter};

    #[test]
    fn test_router_default_mappings() {
        let router = SandboxRouter::new();

        assert_eq!(router.get_sandbox_type(ToolCategory::Shell), SandboxType::Docker);
        assert_eq!(router.get_sandbox_type(ToolCategory::Compute), SandboxType::Wasm);
        assert_eq!(
            router.get_sandbox_type(ToolCategory::FileSystem),
            SandboxType::Docker
        );
        assert_eq!(router.get_sandbox_type(ToolCategory::Network), SandboxType::Wasm);
    }

    #[test]
    fn test_router_set_mapping() {
        let mut router = SandboxRouter::new();

        // Override default mapping
        router.set_mapping(ToolCategory::Shell, SandboxType::Subprocess);
        assert_eq!(router.get_sandbox_type(ToolCategory::Shell), SandboxType::Subprocess);
    }

    #[test]
    fn test_router_set_default() {
        let mut router = SandboxRouter::new();

        // Set custom default
        router.set_default(SandboxType::Wasm);

        // Verify default is set
        let mut router2 = SandboxRouter::new();
        router2.set_default(SandboxType::Wasm);
        // Shell has explicit mapping so still uses Docker
        assert_eq!(router2.get_sandbox_type(ToolCategory::Shell), SandboxType::Docker);
    }

    #[tokio::test]
    async fn test_router_register_adapter() {
        let mut router = SandboxRouter::new();

        // Register adapters
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));
        router.register_adapter(AdapterEnum::Wasm(WasmAdapter::new()));
        router.register_adapter(AdapterEnum::Docker(DockerAdapter::new("alpine:latest")));

        // Get adapter should work
        assert!(router.get_adapter(SandboxType::Subprocess).is_some());
        assert!(router.get_adapter(SandboxType::Wasm).is_some());
        assert!(router.get_adapter(SandboxType::Docker).is_some());
    }

    #[tokio::test]
    async fn test_router_execute() {
        let mut router = SandboxRouter::new();

        // Register adapters
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));

        // Override compute to use subprocess (since Wasm may not be available)
        router.set_mapping(ToolCategory::Compute, SandboxType::Subprocess);

        // Execute with Compute category (now maps to Subprocess)
        let result = router
            .execute(ToolCategory::Compute, "echo 'hello from router'", "bash")
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.stdout.contains("hello from router"));
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_router_execute_unregistered() {
        let router = SandboxRouter::new();

        // Try to execute without registering adapter - should fail
        let result = router
            .execute(ToolCategory::Compute, "echo hello", "bash")
            .await;

        // Should fail because no adapter is registered
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_router_shell_category() {
        let mut router = SandboxRouter::new();

        // Register subprocess adapter
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));

        // Override shell to use subprocess (since Docker may not be available)
        router.set_mapping(ToolCategory::Shell, SandboxType::Subprocess);

        let result = router
            .execute(ToolCategory::Shell, "echo 'shell test'", "bash")
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.stdout.contains("shell test"));
    }

    #[tokio::test]
    async fn test_router_filesystem_category() {
        let mut router = SandboxRouter::new();

        // Register subprocess adapter
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));

        // Override filesystem to use subprocess (since Docker may not be available)
        router.set_mapping(ToolCategory::FileSystem, SandboxType::Subprocess);

        let result = router
            .execute(ToolCategory::FileSystem, "ls /tmp", "bash")
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }
}
