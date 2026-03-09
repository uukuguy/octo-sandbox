//! Integration tests for the WASM skill runtime.
//!
//! These tests verify that `WasmSkillRuntime` correctly implements the
//! `SkillRuntime` trait and integrates with `SkillRuntimeBridge`.
//!
//! Gated behind `sandbox-wasm` feature.

#![cfg(feature = "sandbox-wasm")]

use std::path::PathBuf;

use octo_engine::skill_runtime::{RuntimeType, SkillContext, SkillRuntime, WasmSkillRuntime};
use octo_engine::skills::runtime_bridge::SkillRuntimeBridge;

#[test]
fn test_wasm_runtime_type() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
    assert_eq!(runtime.runtime_type(), RuntimeType::WASM);
}

#[test]
fn test_wasm_runtime_creation() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
    assert_eq!(runtime.runtime_type(), RuntimeType::WASM);
}

#[test]
fn test_wasm_runtime_with_memory_limit() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp")).with_memory_limit(128);
    assert_eq!(runtime.runtime_type(), RuntimeType::WASM);
    // Builder returns self, so we can verify it compiles and the type is correct
}

#[test]
fn test_wasm_runtime_with_fuel_limit() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp")).with_fuel_limit(999);
    assert_eq!(runtime.runtime_type(), RuntimeType::WASM);
}

#[tokio::test]
async fn test_wasm_runtime_check_environment() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
    let result = runtime.check_environment().await;
    assert!(result.is_ok(), "check_environment should pass for /tmp");
}

#[tokio::test]
async fn test_wasm_runtime_check_environment_bad_dir() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/nonexistent/wasm/dir/that/does/not/exist"));
    let result = runtime.check_environment().await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not exist"),
        "Error should mention missing directory, got: {}",
        err_msg
    );
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
    assert!(
        err_msg.contains("WASM runtime"),
        "Error should mention WASM runtime, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_wasm_runtime_execute_wat() {
    let runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
    let context = SkillContext::new("test_wasm_wat".to_string(), PathBuf::from("/tmp"));

    // Minimal WAT module exporting _start -> i32
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

#[test]
fn test_wasm_bridge_integration() {
    let tmp = std::env::temp_dir();
    let mut bridge = SkillRuntimeBridge::new(tmp.clone());

    // Add the WASM runtime
    let wasm_runtime = WasmSkillRuntime::new(PathBuf::from("/tmp"));
    bridge.add_runtime(Box::new(wasm_runtime));

    // Now extension "wasm" should resolve
    let rt = bridge.get_runtime_for_extension("wasm");
    assert!(rt.is_some(), "WASM runtime should be available via bridge");
    assert_eq!(rt.unwrap().runtime_type(), RuntimeType::WASM);

    // Also accessible via get_runtime
    let rt2 = bridge.get_runtime(RuntimeType::WASM);
    assert!(rt2.is_some());
    assert_eq!(rt2.unwrap().runtime_type(), RuntimeType::WASM);
}
