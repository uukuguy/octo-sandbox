//! Sandbox router for routing tools to appropriate sandbox types
//!
//! This module provides a router that maps tool categories to sandbox types
//! and routes execution requests to the appropriate adapter.

use super::{
    ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxPolicy, SandboxType,
};
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
    /// Script execution (Python, Node.js, etc.)
    Script,
    /// GPU-accelerated workloads
    Gpu,
    /// Untrusted code from external sources
    Untrusted,
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
#[derive(Clone)]
pub struct SandboxRouter {
    /// Registered adapters by sandbox type
    adapters: HashMap<SandboxType, Arc<AdapterEnum>>,
    /// Default sandbox type when no mapping found
    default_sandbox: SandboxType,
    /// Tool category to sandbox type mapping
    tool_mapping: HashMap<ToolCategory, SandboxType>,
    /// Sandbox execution policy
    policy: SandboxPolicy,
}

impl SandboxRouter {
    /// Create a new SandboxRouter with default mappings
    ///
    /// Default mappings:
    /// - Shell -> Docker
    /// - Compute -> Wasm
    /// - FileSystem -> Docker
    /// - Network -> Wasm
    /// - Script -> Docker
    /// - Gpu -> Docker
    /// - Untrusted -> Docker
    pub fn new() -> Self {
        Self::with_policy(SandboxPolicy::default())
    }

    /// Create a new SandboxRouter with a specific policy
    pub fn with_policy(policy: SandboxPolicy) -> Self {
        let mut tool_mapping = HashMap::new();
        tool_mapping.insert(ToolCategory::Shell, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Compute, SandboxType::Wasm);
        tool_mapping.insert(ToolCategory::FileSystem, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Network, SandboxType::Wasm);
        tool_mapping.insert(ToolCategory::Script, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Gpu, SandboxType::Docker);
        tool_mapping.insert(ToolCategory::Untrusted, SandboxType::Docker);

        Self {
            adapters: HashMap::new(),
            default_sandbox: SandboxType::Subprocess,
            tool_mapping,
            policy,
        }
    }

    /// Get the current policy
    pub fn policy(&self) -> SandboxPolicy {
        self.policy
    }

    /// Set the sandbox policy
    pub fn set_policy(&mut self, policy: SandboxPolicy) {
        self.policy = policy;
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
            .cloned()
            .unwrap_or_else(|| self.default_sandbox.clone())
    }

    /// Get an adapter by sandbox type
    pub fn get_adapter(&self, sandbox_type: &SandboxType) -> Option<&Arc<AdapterEnum>> {
        self.adapters.get(sandbox_type)
    }

    /// List all registered adapter types
    pub fn registered_backends(&self) -> Vec<SandboxType> {
        self.adapters.keys().cloned().collect()
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
        let target_type = self.get_sandbox_type(category);

        // Try to get the target adapter
        let (actual_type, adapter) = if let Some(adapter) = self.adapters.get(&target_type) {
            (target_type.clone(), adapter)
        } else {
            // Target adapter not available — try fallback
            self.resolve_fallback(&target_type)?
        };

        // Policy enforcement
        if !self.policy.allows(&actual_type) {
            return Err(SandboxError::PolicyDenied {
                policy: self.policy,
                sandbox_type: actual_type,
            });
        }

        // Log degradation if needed
        if self.policy.requires_degradation_audit(&target_type, &actual_type) {
            tracing::warn!(
                "Sandbox degradation: {} -> {} (policy: {})",
                target_type,
                actual_type,
                self.policy
            );
        }

        let config = SandboxConfig::new(actual_type);
        let id = adapter.create(&config).await?;
        let result = adapter.execute(&id, code, language).await;
        let _ = adapter.destroy(&id).await;

        result
    }

    /// Try to find a fallback adapter when the target is not available
    fn resolve_fallback(
        &self,
        target_type: &SandboxType,
    ) -> Result<(SandboxType, &Arc<AdapterEnum>), SandboxError> {
        // Fallback order: Docker -> Wasm -> Subprocess
        let fallback_order = [SandboxType::Docker, SandboxType::Wasm, SandboxType::Subprocess];

        for fallback in &fallback_order {
            if fallback != target_type {
                if let Some(adapter) = self.adapters.get(fallback) {
                    return Ok((fallback.clone(), adapter));
                }
            }
        }

        Err(SandboxError::UnsupportedType(format!(
            "{} adapter not registered and no fallback available",
            target_type
        )))
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

        assert_eq!(
            router.get_sandbox_type(ToolCategory::Shell),
            SandboxType::Docker
        );
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Compute),
            SandboxType::Wasm
        );
        assert_eq!(
            router.get_sandbox_type(ToolCategory::FileSystem),
            SandboxType::Docker
        );
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Network),
            SandboxType::Wasm
        );
    }

    #[test]
    fn test_router_set_mapping() {
        let mut router = SandboxRouter::new();

        // Override default mapping
        router.set_mapping(ToolCategory::Shell, SandboxType::Subprocess);
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Shell),
            SandboxType::Subprocess
        );
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
        assert_eq!(
            router2.get_sandbox_type(ToolCategory::Shell),
            SandboxType::Docker
        );
    }

    #[tokio::test]
    async fn test_router_register_adapter() {
        let mut router = SandboxRouter::new();

        // Register adapters
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));
        router.register_adapter(AdapterEnum::Wasm(WasmAdapter::new()));
        router.register_adapter(AdapterEnum::Docker(DockerAdapter::new("alpine:latest")));

        // Get adapter should work
        assert!(router.get_adapter(&SandboxType::Subprocess).is_some());
        assert!(router.get_adapter(&SandboxType::Wasm).is_some());
        assert!(router.get_adapter(&SandboxType::Docker).is_some());
    }

    #[tokio::test]
    async fn test_router_execute() {
        // Use Development policy to allow Subprocess
        let mut router = SandboxRouter::with_policy(SandboxPolicy::Development);

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
        let mut router = SandboxRouter::with_policy(SandboxPolicy::Development);

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
        let mut router = SandboxRouter::with_policy(SandboxPolicy::Development);

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

    // ── SandboxPolicy tests ──

    #[test]
    fn test_policy_default_is_strict() {
        assert_eq!(SandboxPolicy::default(), SandboxPolicy::Strict);
    }

    #[test]
    fn test_strict_allows_docker_and_wasm() {
        let policy = SandboxPolicy::Strict;
        assert!(policy.allows(&SandboxType::Docker));
        assert!(policy.allows(&SandboxType::Wasm));
        assert!(!policy.allows(&SandboxType::Subprocess));
        assert!(policy.allows(&SandboxType::External("e2b".to_string())));
    }

    #[test]
    fn test_preferred_allows_all() {
        let policy = SandboxPolicy::Preferred;
        assert!(policy.allows(&SandboxType::Docker));
        assert!(policy.allows(&SandboxType::Wasm));
        assert!(policy.allows(&SandboxType::Subprocess));
    }

    #[test]
    fn test_development_allows_all() {
        let policy = SandboxPolicy::Development;
        assert!(policy.allows(&SandboxType::Docker));
        assert!(policy.allows(&SandboxType::Wasm));
        assert!(policy.allows(&SandboxType::Subprocess));
    }

    #[test]
    fn test_preferred_degradation_audit() {
        let policy = SandboxPolicy::Preferred;
        assert!(policy.requires_degradation_audit(&SandboxType::Docker, &SandboxType::Subprocess));
        assert!(!policy.requires_degradation_audit(&SandboxType::Docker, &SandboxType::Docker));
    }

    #[test]
    fn test_strict_no_degradation_audit() {
        let policy = SandboxPolicy::Strict;
        assert!(!policy.requires_degradation_audit(&SandboxType::Docker, &SandboxType::Subprocess));
    }

    #[test]
    fn test_new_category_default_mappings() {
        let router = SandboxRouter::new();
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Script),
            SandboxType::Docker
        );
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Gpu),
            SandboxType::Docker
        );
        assert_eq!(
            router.get_sandbox_type(ToolCategory::Untrusted),
            SandboxType::Docker
        );
    }

    #[tokio::test]
    async fn test_strict_policy_denies_subprocess() {
        let mut router = SandboxRouter::with_policy(SandboxPolicy::Strict);
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));
        router.set_mapping(ToolCategory::Compute, SandboxType::Subprocess);

        let result = router
            .execute(ToolCategory::Compute, "echo test", "bash")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, SandboxError::PolicyDenied { policy: SandboxPolicy::Strict, sandbox_type: SandboxType::Subprocess }),
            "Expected PolicyDenied, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_development_policy_allows_subprocess() {
        let mut router = SandboxRouter::with_policy(SandboxPolicy::Development);
        router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));
        router.set_mapping(ToolCategory::Compute, SandboxType::Subprocess);

        let result = router
            .execute(ToolCategory::Compute, "echo 'dev mode'", "bash")
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().stdout.contains("dev mode"));
    }

    #[test]
    fn test_router_with_policy() {
        let router = SandboxRouter::with_policy(SandboxPolicy::Development);
        assert_eq!(router.policy(), SandboxPolicy::Development);

        let router = SandboxRouter::new();
        assert_eq!(router.policy(), SandboxPolicy::Strict);
    }

    #[test]
    fn test_router_set_policy() {
        let mut router = SandboxRouter::new();
        assert_eq!(router.policy(), SandboxPolicy::Strict);

        router.set_policy(SandboxPolicy::Development);
        assert_eq!(router.policy(), SandboxPolicy::Development);
    }
}
