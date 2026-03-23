//! Sandbox module for secure code execution
//!
//! This module provides sandbox execution environments for running untrusted code
//! with support for multiple runtime types:
//! - WASM: WebAssembly sandbox (lightweight, fast)
//! - Docker: Container-based sandbox (isolation)
//! - Subprocess: Local subprocess execution (simple)

pub mod audit;
pub mod docker;
pub mod external;
pub mod profile;
pub mod router;
pub mod run_mode;
pub mod subprocess;
pub mod target;
pub mod traits;
pub mod wasm;

pub use docker::{DockerAdapter, ImageRegistry};
pub use external::{
    ExecRequest, ExternalSandboxConfig, ExternalSandboxId, ExternalSandboxProvider, StubE2BProvider,
};
pub use profile::{CustomSandboxConfig, SandboxProfile};
pub use router::{AdapterEnum, SandboxRouter, ToolCategory};
pub use run_mode::OctoRunMode;
pub use subprocess::SubprocessAdapter;
pub use target::{ExecutionTarget, ExecutionTargetResolver, RoutingPreview, SandboxRef};
pub use audit::{ResourceUsage, SandboxAction, SandboxAuditEvent};
pub use traits::{
    ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxPolicy, SandboxType,
};
pub use wasm::WasmAdapter;
