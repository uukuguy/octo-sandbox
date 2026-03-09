use std::path::PathBuf;

use octo_engine::skill_runtime::traits::RuntimeType;
use octo_engine::skill_runtime::{NodeJsRuntime, SkillContext, SkillRuntime};

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_nodejs_runtime_type() {
    let runtime = NodeJsRuntime::new();
    assert_eq!(runtime.runtime_type(), RuntimeType::NodeJS);
}

#[tokio::test]
async fn test_nodejs_execute_simple() {
    if !node_available() {
        // Node.js not installed, skipping test
        return;
    }

    let runtime = NodeJsRuntime::new();
    let context = SkillContext::new("test_node".to_string(), PathBuf::from("/tmp"));
    let args = serde_json::json!({});

    let result = runtime
        .execute(
            r#"console.log(JSON.stringify({result: "ok"}))"#,
            args,
            &context,
        )
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val["result"], "ok");
}

#[tokio::test]
async fn test_nodejs_execute_with_args() {
    if !node_available() {
        // Node.js not installed, skipping test
        return;
    }

    let runtime = NodeJsRuntime::new();
    let context = SkillContext::new("test_node_args".to_string(), PathBuf::from("/tmp"));
    let args = serde_json::json!({"name": "world"});

    let script = r#"
const args = JSON.parse(process.env.SKILL_ARGS || '{}');
console.log(JSON.stringify({greeting: `hello ${args.name}`}));
"#;

    let result = runtime.execute(script, args, &context).await;
    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val["greeting"], "hello world");
}

#[tokio::test]
async fn test_nodejs_execute_error() {
    if !node_available() {
        // Node.js not installed, skipping test
        return;
    }

    let runtime = NodeJsRuntime::new();
    let context = SkillContext::new("test_node_err".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute("throw new Error('boom');", serde_json::json!({}), &context)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_nodejs_execute_plain_text() {
    if !node_available() {
        // Node.js not installed, skipping test
        return;
    }

    let runtime = NodeJsRuntime::new();
    let context = SkillContext::new("test_node_text".to_string(), PathBuf::from("/tmp"));

    let result = runtime
        .execute(
            "console.log('just a string');",
            serde_json::json!({}),
            &context,
        )
        .await;

    assert!(result.is_ok(), "execute failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val, serde_json::Value::String("just a string".to_string()));
}

#[tokio::test]
async fn test_nodejs_check_environment() {
    if !node_available() {
        // Node.js not installed, skipping test
        return;
    }

    let runtime = NodeJsRuntime::new();
    let result = runtime.check_environment().await;
    assert!(
        result.is_ok(),
        "check_environment failed: {:?}",
        result.err()
    );
}
