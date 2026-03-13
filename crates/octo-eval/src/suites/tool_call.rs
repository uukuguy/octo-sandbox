//! Tool Call evaluation suite — tests tool selection and argument accuracy.

use std::path::Path;

use anyhow::Result;

use crate::datasets::loader::{load_jsonl, load_jsonl_as_tasks, JsonlTask};
use crate::task::EvalTask;

/// Tool call evaluation suite
pub struct ToolCallSuite;

impl ToolCallSuite {
    /// Default dataset path (relative to crate root)
    const DEFAULT_DATASET: &'static str = "datasets/octo_tool_call.jsonl";

    /// Load tasks from the default dataset
    pub fn load() -> Result<Vec<Box<dyn EvalTask>>> {
        // Try relative to current dir, then relative to crate manifest dir
        let path = Path::new(Self::DEFAULT_DATASET);
        if path.exists() {
            return load_jsonl_as_tasks(path);
        }
        // Try from crate root
        let crate_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(Self::DEFAULT_DATASET);
        if crate_path.exists() {
            return load_jsonl_as_tasks(&crate_path);
        }
        anyhow::bail!(
            "Tool call dataset not found at {} or {}",
            path.display(),
            crate_path.display()
        )
    }

    /// Load tasks from a custom path
    pub fn load_from(path: &Path) -> Result<Vec<Box<dyn EvalTask>>> {
        load_jsonl_as_tasks(path)
    }

    /// Load raw JsonlTask structs (useful for inspection)
    pub fn load_raw() -> Result<Vec<JsonlTask>> {
        let crate_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(Self::DEFAULT_DATASET);
        load_jsonl(&crate_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_tool_call_suite() {
        let tasks = ToolCallSuite::load().unwrap();
        assert!(
            tasks.len() >= 3,
            "Expected at least 3 tool call tasks, got {}",
            tasks.len()
        );

        // Verify first task
        assert_eq!(tasks[0].id(), "tool-001");
        assert!(tasks[0].prompt().contains("Read"));
    }

    #[test]
    fn test_load_raw() {
        let tasks = ToolCallSuite::load_raw().unwrap();
        assert!(tasks.len() >= 3);
        assert_eq!(tasks[0].category, "tool_call");
    }
}
