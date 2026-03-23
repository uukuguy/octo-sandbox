//! ExecutionTarget and routing decision engine
//!
//! Determines where each tool call should execute based on:
//! - OctoRunMode (Sandboxed vs Host)
//! - SandboxProfile (Development/Staging/Production/Custom)
//! - ToolCategory
//! - Available sandbox backends

use serde::{Deserialize, Serialize};
use std::fmt;

use super::profile::SandboxProfile;
use super::router::ToolCategory;
use super::run_mode::OctoRunMode;
use super::traits::SandboxType;

/// Where a tool call should execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// Execute locally (direct subprocess).
    /// Used when: Development mode, or Octo is already sandboxed (Mode A).
    Local,

    /// Execute inside a sandbox.
    Sandbox(SandboxRef),
}

impl fmt::Display for ExecutionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionTarget::Local => write!(f, "local"),
            ExecutionTarget::Sandbox(r) => write!(f, "sandbox:{}", r),
        }
    }
}

/// Reference to a sandbox for execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxRef {
    /// Use a persistent per-session sandbox (container reuse).
    Session { id: String },
    /// Create an ephemeral sandbox for this single execution.
    Ephemeral { sandbox_type: SandboxType },
}

impl fmt::Display for SandboxRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxRef::Session { id } => write!(f, "session:{}", id),
            SandboxRef::Ephemeral { sandbox_type } => write!(f, "ephemeral:{}", sandbox_type),
        }
    }
}

/// Resolves the execution target for a tool call.
///
/// Implements the routing decision matrix:
/// - Mode A (Sandboxed): Always Local (container provides isolation)
/// - Mode B (Host) + Development: Always Local (zero-friction dev)
/// - Mode B (Host) + Staging: Sandbox preferred, Local fallback
/// - Mode B (Host) + Production: Sandbox required (no Local fallback)
#[derive(Debug, Clone)]
pub struct ExecutionTargetResolver {
    run_mode: OctoRunMode,
    profile: SandboxProfile,
    available_backends: Vec<SandboxType>,
}

impl ExecutionTargetResolver {
    pub fn new(
        run_mode: OctoRunMode,
        profile: SandboxProfile,
        available_backends: Vec<SandboxType>,
    ) -> Self {
        Self {
            run_mode,
            profile,
            available_backends,
        }
    }

    /// Get the current run mode.
    pub fn run_mode(&self) -> OctoRunMode {
        self.run_mode
    }

    /// Get the current profile.
    pub fn profile(&self) -> &SandboxProfile {
        &self.profile
    }

    /// Get the list of available backends.
    pub fn available_backends(&self) -> &[SandboxType] {
        &self.available_backends
    }

    /// Resolve the execution target for a given tool category.
    ///
    /// Returns `(ExecutionTarget, routing_reason)`.
    pub fn resolve(&self, category: ToolCategory) -> (ExecutionTarget, String) {
        // Mode A: Octo is already inside a sandbox — execute locally
        if self.run_mode.is_local_execution() {
            return (
                ExecutionTarget::Local,
                "OctoRunMode=Sandboxed: container provides isolation".to_string(),
            );
        }

        // Mode B: Octo is on the host — route based on profile
        match &self.profile {
            SandboxProfile::Development => (
                ExecutionTarget::Local,
                "SandboxProfile=Development: zero-friction local execution".to_string(),
            ),

            SandboxProfile::Staging => self.resolve_staging(category),

            SandboxProfile::Production => self.resolve_production(category),

            SandboxProfile::Custom(config) => {
                // Custom profile routes based on its policy
                use super::traits::SandboxPolicy;
                match config.policy {
                    SandboxPolicy::Development => (
                        ExecutionTarget::Local,
                        "Custom profile with Development policy".to_string(),
                    ),
                    SandboxPolicy::Preferred => self.resolve_staging(category),
                    SandboxPolicy::Strict => self.resolve_production(category),
                }
            }
        }
    }

    /// Staging: prefer sandbox, fall back to local with warning.
    fn resolve_staging(&self, category: ToolCategory) -> (ExecutionTarget, String) {
        let preferred = self.preferred_backend(category);

        if let Some(backend) = preferred {
            (
                ExecutionTarget::Sandbox(SandboxRef::Ephemeral {
                    sandbox_type: backend.clone(),
                }),
                format!(
                    "SandboxProfile=Staging: {} routed to {}",
                    category_name(category),
                    backend
                ),
            )
        } else {
            (
                ExecutionTarget::Local,
                format!(
                    "SandboxProfile=Staging: no backend available for {}, degrading to local",
                    category_name(category)
                ),
            )
        }
    }

    /// Production: require sandbox, no local fallback.
    fn resolve_production(&self, category: ToolCategory) -> (ExecutionTarget, String) {
        let preferred = self.preferred_backend(category);

        if let Some(backend) = preferred {
            (
                ExecutionTarget::Sandbox(SandboxRef::Ephemeral {
                    sandbox_type: backend.clone(),
                }),
                format!(
                    "SandboxProfile=Production: {} routed to {}",
                    category_name(category),
                    backend
                ),
            )
        } else {
            // Production mode with no backend: still return Sandbox target
            // with the default type — the router will fail with a clear error
            (
                ExecutionTarget::Sandbox(SandboxRef::Ephemeral {
                    sandbox_type: self.default_backend_for(category),
                }),
                format!(
                    "SandboxProfile=Production: {} requires sandbox but no backend available",
                    category_name(category)
                ),
            )
        }
    }

    /// Find the best available backend for a tool category.
    fn preferred_backend(&self, category: ToolCategory) -> Option<&SandboxType> {
        let preferred_type = self.default_backend_for(category);

        // Try exact match first
        if let Some(backend) = self.available_backends.iter().find(|b| **b == preferred_type) {
            return Some(backend);
        }

        // Fallback order: Docker -> External -> Wasm -> Subprocess
        let fallback_order = [
            SandboxType::Docker,
            SandboxType::Wasm,
            SandboxType::Subprocess,
        ];

        for fallback in &fallback_order {
            if let Some(backend) = self.available_backends.iter().find(|b| **b == *fallback) {
                return Some(backend);
            }
        }

        // Check for any External backend
        self.available_backends
            .iter()
            .find(|b| matches!(b, SandboxType::External(_)))
    }

    /// Default sandbox type for a tool category (from design document routing matrix).
    fn default_backend_for(&self, category: ToolCategory) -> SandboxType {
        match category {
            ToolCategory::Shell => SandboxType::Docker,
            ToolCategory::Compute => SandboxType::Wasm,
            ToolCategory::FileSystem => SandboxType::Docker,
            ToolCategory::Network => SandboxType::Docker,
            ToolCategory::Script => SandboxType::Docker,
            ToolCategory::Gpu => SandboxType::Docker,
            ToolCategory::Untrusted => SandboxType::Docker,
        }
    }
}

/// Human-readable name for a tool category.
fn category_name(category: ToolCategory) -> &'static str {
    match category {
        ToolCategory::Shell => "Shell",
        ToolCategory::Compute => "Compute",
        ToolCategory::FileSystem => "FileSystem",
        ToolCategory::Network => "Network",
        ToolCategory::Script => "Script",
        ToolCategory::Gpu => "Gpu",
        ToolCategory::Untrusted => "Untrusted",
    }
}

/// Result of a dry-run routing preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPreview {
    pub category: String,
    pub run_mode: String,
    pub profile: String,
    pub target: String,
    pub reason: String,
    pub available_backends: Vec<String>,
}

impl ExecutionTargetResolver {
    /// Dry-run: preview the routing decision without executing.
    pub fn dry_run(&self, category: ToolCategory) -> RoutingPreview {
        let (target, reason) = self.resolve(category);
        RoutingPreview {
            category: category_name(category).to_string(),
            run_mode: self.run_mode.to_string(),
            profile: self.profile.to_string(),
            target: target.to_string(),
            reason,
            available_backends: self
                .available_backends
                .iter()
                .map(|b| b.to_string())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandboxed_mode_always_local() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Sandboxed,
            SandboxProfile::Production,
            vec![SandboxType::Docker],
        );

        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        assert_eq!(target, ExecutionTarget::Local);
        assert!(reason.contains("Sandboxed"));
    }

    #[test]
    fn test_development_always_local() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Development,
            vec![SandboxType::Docker],
        );

        let (target, _) = resolver.resolve(ToolCategory::Shell);
        assert_eq!(target, ExecutionTarget::Local);
    }

    #[test]
    fn test_staging_with_docker() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Staging,
            vec![SandboxType::Docker],
        );

        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        assert!(matches!(target, ExecutionTarget::Sandbox(_)));
        assert!(reason.contains("Staging"));
    }

    #[test]
    fn test_staging_no_backend_degrades() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Staging,
            vec![], // no backends
        );

        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        assert_eq!(target, ExecutionTarget::Local);
        assert!(reason.contains("degrading"));
    }

    #[test]
    fn test_production_with_docker() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Production,
            vec![SandboxType::Docker],
        );

        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        assert!(matches!(target, ExecutionTarget::Sandbox(_)));
        assert!(reason.contains("Production"));
    }

    #[test]
    fn test_production_no_backend_still_sandbox() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Production,
            vec![], // no backends
        );

        let (target, reason) = resolver.resolve(ToolCategory::Shell);
        // Production never degrades to local
        assert!(matches!(target, ExecutionTarget::Sandbox(_)));
        assert!(reason.contains("requires sandbox"));
    }

    #[test]
    fn test_compute_routes_to_wasm() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Staging,
            vec![SandboxType::Wasm, SandboxType::Docker],
        );

        let (target, _) = resolver.resolve(ToolCategory::Compute);
        match target {
            ExecutionTarget::Sandbox(SandboxRef::Ephemeral { sandbox_type }) => {
                assert_eq!(sandbox_type, SandboxType::Wasm);
            }
            _ => panic!("Expected Sandbox(Ephemeral(Wasm))"),
        }
    }

    #[test]
    fn test_compute_falls_back_to_docker() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Staging,
            vec![SandboxType::Docker], // no Wasm
        );

        let (target, _) = resolver.resolve(ToolCategory::Compute);
        match target {
            ExecutionTarget::Sandbox(SandboxRef::Ephemeral { sandbox_type }) => {
                assert_eq!(sandbox_type, SandboxType::Docker);
            }
            _ => panic!("Expected Sandbox(Ephemeral(Docker))"),
        }
    }

    #[test]
    fn test_external_backend() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Production,
            vec![SandboxType::External("e2b".to_string())],
        );

        let (target, _) = resolver.resolve(ToolCategory::Untrusted);
        match target {
            ExecutionTarget::Sandbox(SandboxRef::Ephemeral { sandbox_type }) => {
                assert!(matches!(sandbox_type, SandboxType::External(_)));
            }
            _ => panic!("Expected Sandbox with External backend"),
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(ExecutionTarget::Local.to_string(), "local");
        assert_eq!(
            ExecutionTarget::Sandbox(SandboxRef::Session {
                id: "s1".to_string()
            })
            .to_string(),
            "sandbox:session:s1"
        );
        assert_eq!(
            ExecutionTarget::Sandbox(SandboxRef::Ephemeral {
                sandbox_type: SandboxType::Docker
            })
            .to_string(),
            "sandbox:ephemeral:docker"
        );
    }

    #[test]
    fn test_dry_run_preview() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Development,
            vec![SandboxType::Docker, SandboxType::Subprocess],
        );

        let preview = resolver.dry_run(ToolCategory::Shell);
        assert_eq!(preview.category, "Shell");
        assert_eq!(preview.run_mode, "host");
        assert_eq!(preview.profile, "development");
        assert_eq!(preview.target, "local");
        assert_eq!(preview.available_backends.len(), 2);
    }

    #[test]
    fn test_all_categories_resolve() {
        let resolver = ExecutionTargetResolver::new(
            OctoRunMode::Host,
            SandboxProfile::Staging,
            vec![SandboxType::Docker, SandboxType::Wasm],
        );

        let categories = [
            ToolCategory::Shell,
            ToolCategory::Compute,
            ToolCategory::FileSystem,
            ToolCategory::Network,
            ToolCategory::Script,
            ToolCategory::Gpu,
            ToolCategory::Untrusted,
        ];

        for cat in &categories {
            let (target, reason) = resolver.resolve(*cat);
            assert!(
                matches!(target, ExecutionTarget::Sandbox(_)),
                "Category {:?} should route to sandbox in Staging mode",
                cat
            );
            assert!(!reason.is_empty());
        }
    }
}
