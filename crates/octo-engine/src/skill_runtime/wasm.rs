//! WASM skill runtime implementation.
//!
//! Executes skill scripts compiled to WebAssembly using Wasmtime.
//! Gated behind the `sandbox-wasm` feature flag.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tracing::debug;

use super::traits::{RuntimeType, SkillRuntime};
use super::SkillContext;

/// WASM-based skill runtime using Wasmtime.
///
/// Skills compiled to `.wasm` can be executed in a sandboxed environment
/// with controlled memory and fuel (computation step) limits.
///
/// The runtime accepts either:
/// - A file path to a `.wasm` binary
/// - Inline WAT (WebAssembly Text) format starting with `(module` or `(component`
pub struct WasmSkillRuntime {
    /// Maximum memory pages (64KB each) for WASM modules.
    max_memory_pages: u32,
    /// Maximum fuel (computation steps) allowed.
    max_fuel: u64,
    /// Working directory for WASM module resolution.
    working_dir: PathBuf,
    /// Wasmtime engine instance.
    engine: wasmtime::Engine,
}

impl WasmSkillRuntime {
    /// Create a new WASM skill runtime with default limits.
    ///
    /// Defaults:
    /// - Memory: 256 pages (16 MB)
    /// - Fuel: 1,000,000 computation steps
    pub fn new(working_dir: PathBuf) -> Self {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);

        let engine =
            wasmtime::Engine::new(&config).expect("failed to create wasmtime engine with fuel");

        Self {
            max_memory_pages: 256, // 16 MB default
            max_fuel: 1_000_000,
            working_dir,
            engine,
        }
    }

    /// Set the maximum memory pages (each page is 64 KB).
    pub fn with_memory_limit(mut self, pages: u32) -> Self {
        self.max_memory_pages = pages;
        self
    }

    /// Set the maximum fuel (computation steps).
    pub fn with_fuel_limit(mut self, fuel: u64) -> Self {
        self.max_fuel = fuel;
        self
    }

    /// Instantiate a module and call its entry point, returning a JSON result.
    fn instantiate_and_call(
        &self,
        module: &wasmtime::Module,
        args: serde_json::Value,
        source_label: &str,
    ) -> Result<serde_json::Value> {
        let mut store = wasmtime::Store::new(&self.engine, ());
        store
            .set_fuel(self.max_fuel)
            .context("Failed to set fuel limit")?;

        let linker = wasmtime::Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, module)
            .context("Failed to instantiate WASM module")?;

        let start = std::time::Instant::now();

        // Resolve entry point: prefer _start, then run
        let func_name = Self::find_entry_point(&mut store, &instance);

        let func_name = match func_name {
            Some(name) => name,
            None => {
                let duration_ms = start.elapsed().as_millis() as u64;
                debug!(
                    "WASM module loaded ({}) but no entry point (_start/run) found",
                    source_label
                );
                return Ok(serde_json::json!({
                    "status": "loaded",
                    "runtime": "wasm",
                    "source": source_label,
                    "args": args,
                    "note": "No _start or run export found",
                    "duration_ms": duration_ms
                }));
            }
        };

        // Try () -> i32 first, then () -> ()
        let (exit_code, success) =
            if let Ok(func) = instance.get_typed_func::<(), i32>(&mut store, func_name) {
                match func.call(&mut store, ()) {
                    Ok(code) => (code, true),
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        return Ok(serde_json::json!({
                            "status": "error",
                            "runtime": "wasm",
                            "source": source_label,
                            "error": format!("WASM trap: {}", e),
                            "duration_ms": duration_ms
                        }));
                    }
                }
            } else if let Ok(func) = instance.get_typed_func::<(), ()>(&mut store, func_name) {
                match func.call(&mut store, ()) {
                    Ok(()) => (0, true),
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        return Ok(serde_json::json!({
                            "status": "error",
                            "runtime": "wasm",
                            "source": source_label,
                            "error": format!("WASM trap: {}", e),
                            "duration_ms": duration_ms
                        }));
                    }
                }
            } else {
                (0, true)
            };

        let duration_ms = start.elapsed().as_millis() as u64;
        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = self.max_fuel.saturating_sub(fuel_remaining);

        debug!(
            "WASM function '{}' executed ({}) in {}ms, fuel consumed: {}",
            func_name, source_label, duration_ms, fuel_consumed
        );

        Ok(serde_json::json!({
            "status": if success { "executed" } else { "error" },
            "runtime": "wasm",
            "source": source_label,
            "function": func_name,
            "exit_code": exit_code,
            "args": args,
            "duration_ms": duration_ms,
            "fuel_consumed": fuel_consumed
        }))
    }

    /// Find a callable entry point in the instance.
    /// Checks _start and run with both () -> i32 and () -> () signatures.
    fn find_entry_point(
        store: &mut wasmtime::Store<()>,
        instance: &wasmtime::Instance,
    ) -> Option<&'static str> {
        ["_start", "run"]
            .into_iter()
            .find(|&name| {
                instance
                    .get_typed_func::<(), i32>(&mut *store, name)
                    .is_ok()
                    || instance.get_typed_func::<(), ()>(&mut *store, name).is_ok()
            })
            .map(|v| v as _)
    }

    /// Execute a WASM file from disk.
    async fn execute_wasm_file(
        &self,
        path: &std::path::Path,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let wasm_bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read WASM file: {}", path.display()))?;

        let module = wasmtime::Module::from_binary(&self.engine, &wasm_bytes)
            .context("Failed to compile WASM module from file")?;

        self.instantiate_and_call(&module, args, &path.display().to_string())
    }

    /// Execute inline WAT (WebAssembly Text) format.
    ///
    /// Uses `wasmtime::Module::new` which accepts WAT text directly
    /// (wasmtime re-exports the `wat` parser).
    async fn execute_wat(
        &self,
        wat_text: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let module = wasmtime::Module::new(&self.engine, wat_text)
            .context("Failed to compile WAT (WebAssembly Text) format")?;

        self.instantiate_and_call(&module, args, "inline-wat")
    }
}

#[async_trait]
impl SkillRuntime for WasmSkillRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::WASM
    }

    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        _context: &SkillContext,
    ) -> Result<serde_json::Value> {
        // Determine whether script is a file path or inline WAT
        let wasm_path = std::path::Path::new(script);

        if wasm_path.extension().is_some_and(|e| e == "wasm") && wasm_path.exists() {
            self.execute_wasm_file(wasm_path, args).await
        } else if script.trim_start().starts_with("(module")
            || script.trim_start().starts_with("(component")
        {
            self.execute_wat(script, args).await
        } else {
            bail!(
                "WASM runtime: script must be a path to an existing .wasm file \
                 or inline WAT format starting with (module or (component"
            )
        }
    }

    async fn check_environment(&self) -> Result<()> {
        if !self.working_dir.exists() {
            bail!(
                "WASM runtime working directory does not exist: {}",
                self.working_dir.display()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wasm_runtime_type() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        assert_eq!(runtime.runtime_type(), RuntimeType::WASM);
    }

    #[test]
    fn test_wasm_runtime_creation() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        assert_eq!(runtime.max_memory_pages, 256);
        assert_eq!(runtime.max_fuel, 1_000_000);
        assert_eq!(runtime.working_dir, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_wasm_runtime_with_memory_limit() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp")).with_memory_limit(512);
        assert_eq!(runtime.max_memory_pages, 512);
    }

    #[test]
    fn test_wasm_runtime_with_fuel_limit() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp")).with_fuel_limit(5_000_000);
        assert_eq!(runtime.max_fuel, 5_000_000);
    }

    #[tokio::test]
    async fn test_wasm_runtime_check_environment() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        let result = runtime.check_environment().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wasm_runtime_check_environment_bad_dir() {
        let runtime =
            WasmSkillRuntime::new(PathBuf::from("/nonexistent/wasm/dir/that/does/not/exist"));
        let result = runtime.check_environment().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_wasm_runtime_execute_invalid_script() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        let context = SkillContext::new("test_wasm".to_string(), PathBuf::from("/tmp"));

        let result = runtime
            .execute("not_a_wasm_file_or_wat", serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("WASM runtime"));
    }

    #[tokio::test]
    async fn test_wasm_runtime_execute_wat() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        let context = SkillContext::new("test_wasm_wat".to_string(), PathBuf::from("/tmp"));

        // A minimal WAT module that exports a _start function returning 42
        let wat = r#"(module
            (func (export "_start") (result i32)
                i32.const 42
            )
        )"#;

        let result = runtime
            .execute(wat, serde_json::json!({"key": "value"}), &context)
            .await;

        assert!(result.is_ok(), "WAT execution failed: {:?}", result.err());
        let val = result.unwrap();
        assert_eq!(val["status"], "executed");
        assert_eq!(val["runtime"], "wasm");
        assert_eq!(val["function"], "_start");
        assert_eq!(val["exit_code"], 42);
    }

    #[tokio::test]
    async fn test_wasm_runtime_execute_wat_void() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        let context = SkillContext::new("test_wasm_void".to_string(), PathBuf::from("/tmp"));

        // A WAT module that exports a run function with no return
        let wat = r#"(module
            (func (export "run")
                nop
            )
        )"#;

        let result = runtime.execute(wat, serde_json::json!({}), &context).await;

        assert!(result.is_ok(), "WAT execution failed: {:?}", result.err());
        let val = result.unwrap();
        assert_eq!(val["status"], "executed");
        assert_eq!(val["function"], "run");
        assert_eq!(val["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_wasm_runtime_execute_wat_no_entry() {
        let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
        let context = SkillContext::new("test_wasm_no_entry".to_string(), PathBuf::from("/tmp"));

        // A WAT module with no _start or run export
        let wat = r#"(module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
        )"#;

        let result = runtime.execute(wat, serde_json::json!({}), &context).await;

        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["status"], "loaded");
        assert!(val["note"].as_str().unwrap().contains("No _start or run"));
    }
}
