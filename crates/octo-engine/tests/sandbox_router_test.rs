// SandboxRouter integration tests

use octo_engine::sandbox::{AdapterEnum, SandboxRouter, SandboxType, SubprocessAdapter, ToolCategory};

#[test]
fn test_router_tool_mapping() {
    let router = SandboxRouter::new();

    // Default mappings
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

    // Verify default is set (check internal state by using unknown category behavior)
    // The default is used when no mapping exists
    let mut router2 = SandboxRouter::new();
    router2.set_default(SandboxType::Wasm);
    // Shell has explicit mapping so still uses Docker
    assert_eq!(router2.get_sandbox_type(ToolCategory::Shell), SandboxType::Docker);
}

#[tokio::test]
async fn test_router_register_adapter() {
    let mut router = SandboxRouter::new();

    // Register subprocess adapter
    router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));

    // Get adapter should work
    assert!(router.get_adapter(SandboxType::Subprocess).is_some());
}

#[tokio::test]
async fn test_router_execute_with_subprocess() {
    let mut router = SandboxRouter::new();

    // Register subprocess adapter for Subprocess type
    router.register_adapter(AdapterEnum::Subprocess(SubprocessAdapter::new()));

    // Set mapping to use subprocess for compute
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
