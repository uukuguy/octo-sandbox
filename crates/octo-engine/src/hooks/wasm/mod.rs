//! WASM Component Model hook plugin system.
//!
//! Allows users to write hook handlers in any language that compiles to WASM
//! components via the Component Model + WIT interface. Plugins are discovered
//! from `~/.octo/plugins/` and `$PROJECT/.octo/plugins/`, loaded at runtime,
//! and executed as `HookHandler` implementations.

#[cfg(feature = "sandbox-wasm")]
pub mod handler;
#[cfg(feature = "sandbox-wasm")]
pub mod host_impl;
#[cfg(feature = "sandbox-wasm")]
pub mod loader;
#[cfg(feature = "sandbox-wasm")]
pub mod manifest;

/// Generated bindings from the `octo-hook.wit` interface via `wasmtime::component::bindgen!`.
///
/// This produces:
/// - `octo::hook::host::Host` trait (host-side implementation)
/// - `OctoHookPlugin` struct (guest component instantiation)
/// - `hook_handler::HookHandler` guest export interface
#[cfg(feature = "sandbox-wasm")]
pub mod bindings {
    wasmtime::component::bindgen!({
        world: "octo-hook-plugin",
        path: "wit/octo-hook.wit",
    });
}
