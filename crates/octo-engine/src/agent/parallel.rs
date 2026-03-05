//! Parallel Tool Execution - Execute multiple tools concurrently
//!
//! This module provides parallel execution capabilities for agent tool calls,
//! allowing multiple tools to run concurrently with a configurable maximum parallelism.

use std::sync::Arc;

use futures_util::future::join_all;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use octo_types::{ToolContext, ToolResult};

use crate::agent::cancellation::CancellationToken;
use crate::tools::ToolRegistry;

/// Execute multiple tools in parallel with a semaphore-controlled concurrency limit.
///
/// # Arguments
/// * `tools` - Vector of (tool_name, input) tuples to execute
/// * `registry` - Tool registry to look up and execute tools
/// * `max_parallel` - Maximum number of tools to run concurrently
/// * `cancellation` - Cancellation token to check for cancellation
/// * `tool_ctx` - Tool execution context (sandbox_id, working_dir, etc.)
///
/// # Returns
/// Vector of (tool_name, ToolResult) tuples in the same order as input
pub async fn execute_parallel(
    tools: Vec<(String, serde_json::Value)>,
    registry: &Arc<ToolRegistry>,
    max_parallel: u8,
    cancellation: &CancellationToken,
    tool_ctx: &ToolContext,
) -> Vec<(String, ToolResult)> {
    if tools.is_empty() {
        return vec![];
    }

    let semaphore = Arc::new(Semaphore::new(max_parallel as usize));

    debug!(
        count = tools.len(),
        max_parallel, "Starting parallel tool execution"
    );

    let tasks: Vec<_> = tools
        .into_iter()
        .map(|(name, input)| {
            let registry = registry.clone();
            let sem = semaphore.clone();
            let cancel = cancellation.child();
            let ctx = tool_ctx.clone();

            async move {
                // Acquire permit from semaphore (non-blocking)
                let _permit = sem.acquire_owned().await.expect("Semaphore closed");

                // Check cancellation before execution
                if cancel.is_cancelled() {
                    warn!(tool = %name, "Tool cancelled before execution");
                    return (name, ToolResult::error("Cancelled by parent"));
                }

                // Execute the tool
                let result = if let Some(tool) = registry.get(&name) {
                    match tool.execute(input, &ctx).await {
                        Ok(r) => r,
                        Err(e) => {
                            warn!(tool = %name, error = %e, "Tool execution failed");
                            ToolResult::error(format!("Tool error: {e}"))
                        }
                    }
                } else {
                    warn!(tool = %name, "Tool not found in registry");
                    ToolResult::error(format!("Unknown tool: {}", name))
                };

                debug!(tool = %name, "Tool execution completed");
                (name, result)
            }
        })
        .collect();

    // Wait for all tasks to complete
    let results = join_all(tasks).await;

    debug!(count = results.len(), "Parallel tool execution completed");

    results
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;

    use super::*;
    use crate::tools::Tool;
    use octo_types::{SandboxId, ToolSource, ToolSpec};

    /// Simple test tool implementation
    #[derive(Debug)]
    struct TestTool {
        name: String,
        description: String,
    }

    impl TestTool {
        fn new(name: &str, description: &str) -> Self {
            Self {
                name: name.to_string(),
                description: description.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            })
        }

        async fn execute(
            &self,
            params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolResult> {
            // Add a small delay to simulate work
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            if let Some(text) = params.get("text").and_then(|v| v.as_str()) {
                Ok(ToolResult::success(format!("echo: {}", text)))
            } else {
                Ok(ToolResult::success("executed"))
            }
        }

        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.name.clone(),
                description: self.description.clone(),
                input_schema: self.parameters(),
            }
        }
    }

    /// Create a test tool context
    fn test_tool_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: SandboxId::from_string("test-sandbox"),
            working_dir: std::path::PathBuf::from("/tmp"),
        }
    }

    /// Create a test registry with test tools
    fn test_registry() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool::new("echo", "Echo tool"));
        registry.register(TestTool::new("delay", "Delay tool"));
        Arc::new(registry)
    }

    #[tokio::test]
    async fn test_execute_parallel_empty() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        let results = execute_parallel(vec![], &registry, 4, &cancellation, &ctx).await;

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_parallel_single() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        let tools = vec![("echo".to_string(), serde_json::json!({"text": "hello"}))];
        let results = execute_parallel(tools, &registry, 4, &cancellation, &ctx).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "echo");
        assert!(!results[0].1.is_error);
    }

    #[tokio::test]
    async fn test_execute_parallel_multiple() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        let tools = vec![
            ("echo".to_string(), serde_json::json!({"text": "hello"})),
            ("echo".to_string(), serde_json::json!({"text": "world"})),
        ];
        let results = execute_parallel(tools, &registry, 4, &cancellation, &ctx).await;

        assert_eq!(results.len(), 2);
        assert!(!results[0].1.is_error);
        assert!(!results[1].1.is_error);
    }

    #[tokio::test]
    async fn test_execute_parallel_unknown_tool() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        let tools = vec![("unknown".to_string(), serde_json::json!({}))];
        let results = execute_parallel(tools, &registry, 4, &cancellation, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error);
        assert!(results[0].1.output.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_execute_parallel_cancellation() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        // Cancel immediately
        cancellation.cancel();

        let tools = vec![("echo".to_string(), serde_json::json!({"text": "hello"}))];
        let results = execute_parallel(tools, &registry, 4, &cancellation, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_error);
        assert!(results[0].1.output.contains("Cancelled"));
    }

    #[tokio::test]
    async fn test_execute_parallel_semaphore_limit() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        // Test with max_parallel = 1 (should process sequentially)
        let tools = vec![
            ("echo".to_string(), serde_json::json!({"text": "a"})),
            ("echo".to_string(), serde_json::json!({"text": "b"})),
        ];
        let results = execute_parallel(tools, &registry, 1, &cancellation, &ctx).await;

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_execute_parallel_preserves_order() {
        let registry = test_registry();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        // Test that results preserve input order
        let tools = vec![
            ("echo".to_string(), serde_json::json!({"text": "first"})),
            ("echo".to_string(), serde_json::json!({"text": "second"})),
            ("echo".to_string(), serde_json::json!({"text": "third"})),
        ];
        let results = execute_parallel(tools, &registry, 4, &cancellation, &ctx).await;

        assert_eq!(results[0].0, "echo");
        assert!(results[0].1.output.contains("first"));
        assert_eq!(results[1].0, "echo");
        assert!(results[1].1.output.contains("second"));
        assert_eq!(results[2].0, "echo");
        assert!(results[2].1.output.contains("third"));
    }

    /// Delay tool for timing tests - accepts "ms" parameter for delay duration
    #[derive(Debug)]
    struct DelayTool;

    impl DelayTool {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl Tool for DelayTool {
        fn name(&self) -> &str {
            "delay"
        }

        fn description(&self) -> &str {
            "Delay tool for testing"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "ms": { "type": "number", "description": "Delay in milliseconds" }
                },
                "required": ["ms"]
            })
        }

        async fn execute(
            &self,
            params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolResult> {
            let ms = params
                .get("ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(10);
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            Ok(ToolResult::success(format!("delayed {}ms", ms)))
        }

        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.name().to_string(),
                description: self.description().to_string(),
                input_schema: self.parameters(),
            }
        }
    }

    fn test_registry_with_delay() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool::new("echo", "Echo tool"));
        registry.register(DelayTool::new());
        Arc::new(registry)
    }

    #[tokio::test]
    async fn test_execute_parallel_is_faster_than_sequential() {
        let registry = test_registry_with_delay();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        // Two tools each taking 100ms
        let tools = vec![
            ("delay".to_string(), serde_json::json!({"ms": 100})),
            ("delay".to_string(), serde_json::json!({"ms": 100})),
        ];

        // Parallel execution with max_parallel=2
        let start = std::time::Instant::now();
        let results = execute_parallel(tools, &registry, 2, &cancellation, &ctx).await;
        let parallel_duration = start.elapsed().as_millis();

        // Should complete in roughly 100ms (parallel) not 200ms (sequential)
        // Allow some overhead but should be well under 150ms
        assert!(
            parallel_duration < 150,
            "Parallel execution took {}ms, expected < 150ms",
            parallel_duration
        );

        assert_eq!(results.len(), 2);
        assert!(!results[0].1.is_error);
        assert!(!results[1].1.is_error);
    }

    #[tokio::test]
    async fn test_execute_parallel_semaphore_limits_concurrency() {
        let registry = test_registry_with_delay();
        let cancellation = CancellationToken::new();
        let ctx = test_tool_ctx();

        // Three tools each taking 100ms, but semaphore limits to 1
        let tools = vec![
            ("delay".to_string(), serde_json::json!({"ms": 100})),
            ("delay".to_string(), serde_json::json!({"ms": 100})),
            ("delay".to_string(), serde_json::json!({"ms": 100})),
        ];

        // Parallel execution with max_parallel=1 (should be sequential)
        let start = std::time::Instant::now();
        let results = execute_parallel(tools, &registry, 1, &cancellation, &ctx).await;
        let duration = start.elapsed().as_millis();

        // Should take at least 300ms (sequential)
        assert!(
            duration >= 290,
            "With semaphore=1, execution took {}ms, expected >= 290ms",
            duration
        );

        assert_eq!(results.len(), 3);
    }
}
