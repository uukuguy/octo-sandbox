use std::path::PathBuf;

use octo_engine::skill_runtime::traits::RuntimeType;
use octo_engine::skill_runtime::{ShellRuntime, SkillContext, SkillRuntime};

#[test]
fn test_shell_runtime_type() {
    let runtime = ShellRuntime::new();
    assert_eq!(runtime.runtime_type(), RuntimeType::Shell);
}

#[tokio::test]
async fn test_shell_execute_echo() {
    let runtime = ShellRuntime::new();
    let context = SkillContext::new("test_shell".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute("echo hello", serde_json::json!({}), &context)
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val, serde_json::Value::String("hello".to_string()));
}

#[tokio::test]
async fn test_shell_execute_json() {
    let runtime = ShellRuntime::new();
    let context = SkillContext::new("test_shell_json".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute(r#"echo '{"key":"val"}'"#, serde_json::json!({}), &context)
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val["key"], "val");
}

#[tokio::test]
async fn test_shell_execute_with_env() {
    let runtime = ShellRuntime::new();
    let context = SkillContext::new("my_skill".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute("echo $SKILL_NAME", serde_json::json!({}), &context)
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val, serde_json::Value::String("my_skill".to_string()));
}

#[tokio::test]
async fn test_shell_execute_failure() {
    let runtime = ShellRuntime::new();
    let context = SkillContext::new("test_shell_fail".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute("exit 1", serde_json::json!({}), &context)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_shell_check_environment() {
    let runtime = ShellRuntime::new();
    let result = runtime.check_environment().await;
    assert!(
        result.is_ok(),
        "check_environment failed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_shell_empty_output() {
    let runtime = ShellRuntime::new();
    let context = SkillContext::new("test_shell_empty".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute("true", serde_json::json!({}), &context)
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val, serde_json::Value::Null);
}
