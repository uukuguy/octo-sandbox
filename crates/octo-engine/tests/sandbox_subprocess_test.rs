// SubprocessAdapter integration tests

use octo_engine::sandbox::{RuntimeAdapter, SandboxConfig, SandboxId, SandboxType, SubprocessAdapter};

#[tokio::test]
async fn test_subprocess_create() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    let id = adapter.create(&config).await.unwrap();

    // Verify sandbox was created
    assert!(!id.to_string().is_empty());

    // Clean up
    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_create_and_execute() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    // Create sandbox
    let id = adapter.create(&config).await.unwrap();

    // Execute command
    let result = adapter
        .execute(&id, "echo 'hello'", "bash")
        .await
        .unwrap();

    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
    assert!(result.success);
    assert!(result.execution_time_ms > 0);

    // Destroy sandbox
    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_execute_stderr() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    let id = adapter.create(&config).await.unwrap();

    // Execute command that writes to stderr
    let result = adapter
        .execute(&id, "echo 'error message' >&2", "bash")
        .await
        .unwrap();

    assert_eq!(result.stderr.trim(), "error message");
    assert_eq!(result.exit_code, 0);

    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_failed_command() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    let id = adapter.create(&config).await.unwrap();

    // Execute command that fails
    let result = adapter
        .execute(&id, "exit 1", "bash")
        .await
        .unwrap();

    assert_eq!(result.exit_code, 1);
    assert!(!result.success);

    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_not_found() {
    let adapter = SubprocessAdapter::new();
    let fake_id = SandboxId::new("non-existent-id");

    let result = adapter.execute(&fake_id, "echo test", "bash").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_subprocess_destroy_not_found() {
    let adapter = SubprocessAdapter::new();
    let fake_id = SandboxId::new("non-existent-id");

    // Destroying a non-existent sandbox should not fail
    let result = adapter.destroy(&fake_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_subprocess_with_env_vars() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess)
        .with_env("TEST_VAR", "test_value");

    let id = adapter.create(&config).await.unwrap();

    // Execute command that uses the environment variable
    let result = adapter
        .execute(&id, "echo $TEST_VAR", "bash")
        .await
        .unwrap();

    assert_eq!(result.stdout.trim(), "test_value");

    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_working_directory() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    let id = adapter.create(&config).await.unwrap();

    // Create a file in the working directory and verify it exists
    adapter
        .execute(&id, "touch test_file.txt && ls test_file.txt", "bash")
        .await
        .unwrap();

    // Verify the file was created in the working directory
    let result = adapter
        .execute(&id, "ls test_file.txt", "bash")
        .await
        .unwrap();

    assert!(result.stdout.contains("test_file.txt"));
    assert!(result.success);

    adapter.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_subprocess_multiple_instances() {
    let adapter = SubprocessAdapter::new();
    let config = SandboxConfig::new(SandboxType::Subprocess);

    // Create multiple sandbox instances
    let id1 = adapter.create(&config).await.unwrap();
    let id2 = adapter.create(&config).await.unwrap();
    let id3 = adapter.create(&config).await.unwrap();

    // Execute in each
    let result1 = adapter
        .execute(&id1, "echo 'instance1'", "bash")
        .await
        .unwrap();
    let result2 = adapter
        .execute(&id2, "echo 'instance2'", "bash")
        .await
        .unwrap();
    let result3 = adapter
        .execute(&id3, "echo 'instance3'", "bash")
        .await
        .unwrap();

    assert!(result1.stdout.contains("instance1"));
    assert!(result2.stdout.contains("instance2"));
    assert!(result3.stdout.contains("instance3"));

    // Clean up all
    adapter.destroy(&id1).await.unwrap();
    adapter.destroy(&id2).await.unwrap();
    adapter.destroy(&id3).await.unwrap();
}
