//! AV-T6: Tests for streaming tool execution.

use std::sync::Arc;

use octo_engine::agent::StreamingToolExecutor;
use octo_engine::tools::ToolRegistry;
use octo_types::ToolContext;
use serde_json::json;

fn make_registry() -> Arc<ToolRegistry> {
    // Default ToolRegistry with built-in tools
    Arc::new(ToolRegistry::new())
}

fn make_ctx() -> ToolContext {
    ToolContext {
        sandbox_id: Default::default(),
        user_id: Default::default(),
        working_dir: std::path::PathBuf::from("/tmp"),
        path_validator: None,
    }
}

#[test]
fn test_streaming_executor_creation() {
    let registry = make_registry();
    let ctx = make_ctx();
    let executor = StreamingToolExecutor::new(registry, ctx);
    assert_eq!(executor.pending_count(), 0);
    assert_eq!(executor.spawned_count(), 0);
    assert_eq!(executor.queued_count(), 0);
}

#[test]
fn test_streaming_executor_queues_unknown_tool() {
    let registry = make_registry();
    let ctx = make_ctx();
    let mut executor = StreamingToolExecutor::new(registry, ctx);

    // Unknown tool should be queued (treated as unsafe since get() returns None)
    executor.on_tool_block_complete("call_1", "unknown_tool_xyz", json!({}));
    assert_eq!(executor.pending_count(), 1);
    assert_eq!(executor.queued_count(), 1);
    assert_eq!(executor.spawned_count(), 0);
}

#[tokio::test]
async fn test_streaming_executor_finalize_empty() {
    let registry = make_registry();
    let ctx = make_ctx();
    let executor = StreamingToolExecutor::new(registry, ctx);

    let results = executor.finalize().await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_streaming_executor_finalize_unknown_tool() {
    let registry = make_registry();
    let ctx = make_ctx();
    let mut executor = StreamingToolExecutor::new(registry, ctx);

    executor.on_tool_block_complete("call_1", "nonexistent_tool", json!({"x": 1}));

    let results = executor.finalize().await;
    assert_eq!(results.len(), 1);
    let (id, name, output) = &results[0];
    assert_eq!(id, "call_1");
    assert_eq!(name, "nonexistent_tool");
    assert!(output.is_error);
    assert!(output.content.contains("Unknown tool"));
}

#[test]
fn test_streaming_executor_discard() {
    let registry = make_registry();
    let ctx = make_ctx();
    let executor = StreamingToolExecutor::new(registry, ctx);
    // Should not panic
    executor.discard();
}

#[test]
fn test_streaming_executor_multiple_queued_tools() {
    let registry = make_registry();
    let ctx = make_ctx();
    let mut executor = StreamingToolExecutor::new(registry, ctx);

    executor.on_tool_block_complete("call_1", "tool_a", json!({}));
    executor.on_tool_block_complete("call_2", "tool_b", json!({}));
    executor.on_tool_block_complete("call_3", "tool_c", json!({}));

    assert_eq!(executor.pending_count(), 3);
    assert_eq!(executor.queued_count(), 3);
    assert_eq!(executor.spawned_count(), 0);
}
