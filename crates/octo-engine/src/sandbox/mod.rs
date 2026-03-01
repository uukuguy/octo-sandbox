//! Sandbox module for secure code execution
//!
//! This module provides sandbox execution environments for running untrusted code
//! with support for multiple runtime types:
//! - WASM: WebAssembly sandbox (lightweight, fast)
//! - Docker: Container-based sandbox (isolation)
//! - Subprocess: Local subprocess execution (simple)

pub mod docker;
pub mod router;
pub mod subprocess;
pub mod traits;
pub mod wasm;

pub use docker::DockerAdapter;
pub use router::{AdapterEnum, SandboxRouter, ToolCategory};
pub use subprocess::SubprocessAdapter;
pub use traits::{
    ExecResult, RuntimeAdapter, SandboxConfig, SandboxError, SandboxId, SandboxType,
};
pub use wasm::WasmAdapter;
