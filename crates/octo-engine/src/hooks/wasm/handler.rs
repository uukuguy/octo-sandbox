//! WasmHookHandler — adapter from WASM component to HookHandler trait.
//!
//! Each loaded WASM plugin becomes a `WasmHookHandler` instance that implements
//! the `HookHandler` trait and can be registered in the `HookRegistry`.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use super::host_impl::HookHostState;
use super::manifest::PluginManifest;
use crate::hooks::{HookAction, HookContext, HookFailureMode, HookHandler};

/// A hook handler backed by a WASM component plugin.
#[cfg(feature = "sandbox-wasm")]
pub struct WasmHookHandler {
    /// Wasmtime engine (shared across all plugins).
    engine: wasmtime::Engine,
    /// Pre-compiled WASM component.
    component: wasmtime::component::Component,
    /// Plugin manifest with metadata and capabilities.
    manifest: Arc<PluginManifest>,
}

#[cfg(feature = "sandbox-wasm")]
impl WasmHookHandler {
    /// Create a new WasmHookHandler from a compiled component and manifest.
    pub fn new(
        engine: wasmtime::Engine,
        component: wasmtime::component::Component,
        manifest: PluginManifest,
    ) -> Self {
        Self {
            engine,
            component,
            manifest: Arc::new(manifest),
        }
    }

    /// Load a plugin from its manifest and wasm file path.
    pub fn load(engine: &wasmtime::Engine, manifest: PluginManifest, wasm_path: &std::path::Path) -> anyhow::Result<Self> {
        let wasm_bytes = std::fs::read(wasm_path)?;
        let component = wasmtime::component::Component::from_binary(engine, &wasm_bytes)?;
        Ok(Self::new(engine.clone(), component, manifest))
    }

    /// Parse a HookDecision JSON string into a HookAction.
    fn parse_decision(json: &str) -> anyhow::Result<HookAction> {
        #[derive(serde::Deserialize)]
        struct HookDecision {
            decision: String,
            #[serde(default)]
            reason: Option<String>,
        }

        let decision: HookDecision = serde_json::from_str(json)?;
        match decision.decision.as_str() {
            "allow" | "continue" => Ok(HookAction::Continue),
            "deny" | "abort" => {
                let reason = decision.reason.unwrap_or_else(|| "Denied by WASM plugin".to_string());
                Ok(HookAction::Abort(reason))
            }
            "block" => {
                let reason = decision.reason.unwrap_or_else(|| "Blocked by WASM plugin".to_string());
                Ok(HookAction::Block(reason))
            }
            "ask" => {
                let reason = decision.reason.unwrap_or_else(|| "Needs confirmation".to_string());
                Ok(HookAction::Block(reason))
            }
            other => anyhow::bail!("Unknown decision type: {}", other),
        }
    }
}

#[cfg(feature = "sandbox-wasm")]
#[async_trait]
impl HookHandler for WasmHookHandler {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn priority(&self) -> u32 {
        // Default priority; can be overridden by calling the WASM `priority()` export
        100
    }

    fn failure_mode(&self) -> HookFailureMode {
        self.manifest.hook_failure_mode()
    }

    async fn execute(&self, context: &HookContext) -> anyhow::Result<HookAction> {
        let capabilities: HashSet<String> = self.manifest.capabilities.iter().cloned().collect();

        let host_state = HookHostState::new(
            context.clone(),
            capabilities,
            self.manifest.name.clone(),
        );

        let mut store = wasmtime::Store::new(&self.engine, host_state);

        let mut linker = wasmtime::component::Linker::new(&self.engine);
        super::bindings::OctoHookPlugin::add_to_linker::<
            HookHostState,
            wasmtime::component::HasSelf<HookHostState>,
        >(&mut linker, |state| state)?;

        let plugin =
            super::bindings::OctoHookPlugin::instantiate(&mut store, &self.component, &linker)?;

        // Serialize context to JSON for the guest
        let context_json = serde_json::to_string(context)?;

        // Call the guest's execute function
        let hook_handler = plugin.octo_hook_hook_handler();
        let result = hook_handler.call_execute(&mut store, &context_json)?;

        match result {
            Ok(decision_json) => Self::parse_decision(&decision_json),
            Err(error_msg) => {
                tracing::warn!(
                    plugin = %self.manifest.name,
                    "WASM plugin returned error: {}",
                    error_msg
                );
                anyhow::bail!("WASM plugin error: {}", error_msg)
            }
        }
    }
}

#[cfg(feature = "sandbox-wasm")]
impl std::fmt::Debug for WasmHookHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmHookHandler")
            .field("name", &self.manifest.name)
            .field("version", &self.manifest.version)
            .finish()
    }
}
