//! AV-T6: Streaming Tool Executor
//!
//! Executes safe (concurrency-safe) tools immediately as their tool_use blocks
//! complete during API streaming, without waiting for the full response.
//! Unsafe tools are queued and executed serially after the stream completes.

use std::sync::Arc;

use serde_json::Value;
use tokio::task::JoinHandle;
use tracing::info;

use crate::tools::ToolRegistry;
use octo_types::{ToolContext, ToolOutput};

/// State for a tool that arrived during streaming.
enum ToolState {
    /// Safe tool: execution spawned immediately, handle stored.
    Spawned {
        handle: JoinHandle<(String, ToolOutput)>,
    },
    /// Unsafe tool: queued for serial execution after stream completes.
    Queued { name: String, input: Value },
}

/// Executes safe tools during API streaming, queues unsafe for post-stream serial execution.
pub struct StreamingToolExecutor {
    registry: Arc<ToolRegistry>,
    tool_ctx: ToolContext,
    /// Tools in order of arrival, preserving original sequence for result merging.
    tools: Vec<(String, ToolState)>, // (tool_use_id, state)
}

impl StreamingToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>, ctx: ToolContext) -> Self {
        Self {
            registry,
            tool_ctx: ctx,
            tools: Vec::new(),
        }
    }

    /// Called when a tool_use content block completes during API streaming.
    ///
    /// If the tool is concurrency-safe, execution starts immediately in a spawned task.
    /// Otherwise, the tool is queued for serial execution after the stream ends.
    pub fn on_tool_block_complete(&mut self, tool_use_id: &str, tool_name: &str, input: Value) {
        let is_safe = self
            .registry
            .get(tool_name)
            .map(|t| t.is_concurrency_safe())
            .unwrap_or(false);

        let state = if is_safe {
            // Spawn immediately
            let registry = self.registry.clone();
            let ctx = self.tool_ctx.clone();
            let name = tool_name.to_string();
            let inp = input;
            let handle = tokio::spawn(async move {
                match registry.get(&name) {
                    Some(tool) => match tool.execute(inp, &ctx).await {
                        Ok(output) => (name, output),
                        Err(e) => (name, ToolOutput::error(format!("{e}"))),
                    },
                    None => {
                        let msg = format!("Unknown tool: {name}");
                        (name, ToolOutput::error(msg))
                    }
                }
            });
            info!(
                tool = tool_name,
                "Streaming executor: safe tool spawned immediately"
            );
            ToolState::Spawned { handle }
        } else {
            info!(
                tool = tool_name,
                "Streaming executor: unsafe tool queued for serial execution"
            );
            ToolState::Queued {
                name: tool_name.to_string(),
                input,
            }
        };

        self.tools.push((tool_use_id.to_string(), state));
    }

    /// How many tools are pending (spawned or queued).
    pub fn pending_count(&self) -> usize {
        self.tools.len()
    }

    /// How many safe tools were spawned immediately.
    pub fn spawned_count(&self) -> usize {
        self.tools
            .iter()
            .filter(|(_, s)| matches!(s, ToolState::Spawned { .. }))
            .count()
    }

    /// How many unsafe tools are queued.
    pub fn queued_count(&self) -> usize {
        self.tools
            .iter()
            .filter(|(_, s)| matches!(s, ToolState::Queued { .. }))
            .count()
    }

    /// Finalize: wait for spawned tools, execute queued tools serially, return in order.
    ///
    /// Returns `Vec<(tool_use_id, tool_name, ToolOutput)>`.
    pub async fn finalize(self) -> Vec<(String, String, ToolOutput)> {
        let total = self.tools.len();
        let mut results: Vec<Option<(String, String, ToolOutput)>> = vec![None; total];

        // Separate handles and queued items, remembering their indices
        let mut handles: Vec<(usize, String, JoinHandle<(String, ToolOutput)>)> = Vec::new();
        let mut queued: Vec<(usize, String, String, Value)> = Vec::new();

        let registry = self.registry;
        let tool_ctx = self.tool_ctx;

        for (i, (id, state)) in self.tools.into_iter().enumerate() {
            match state {
                ToolState::Spawned { handle } => {
                    handles.push((i, id, handle));
                }
                ToolState::Queued { name, input } => {
                    queued.push((i, id, name, input));
                }
            }
        }

        // Wait for all spawned (safe) tools
        for (idx, id, handle) in handles {
            match handle.await {
                Ok((name, output)) => {
                    results[idx] = Some((id, name, output));
                }
                Err(e) => {
                    results[idx] =
                        Some((id, "unknown".into(), ToolOutput::error(format!("Task join error: {e}"))));
                }
            }
        }

        // Execute queued (unsafe) tools serially
        for (idx, id, name, input) in queued {
            let output = match registry.get(&name) {
                Some(tool) => match tool.execute(input, &tool_ctx).await {
                    Ok(output) => output,
                    Err(e) => ToolOutput::error(format!("{e}")),
                },
                None => ToolOutput::error(format!("Unknown tool: {name}")),
            };
            results[idx] = Some((id, name, output));
        }

        info!(total, "Streaming executor: all tools finalized");

        results.into_iter().flatten().collect()
    }

    /// Discard all pending work (API stream failed).
    pub fn discard(self) {
        for (_, state) in self.tools {
            if let ToolState::Spawned { handle } = state {
                handle.abort();
            }
        }
    }
}
