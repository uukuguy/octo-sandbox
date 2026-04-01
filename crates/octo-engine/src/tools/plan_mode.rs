//! Plan mode tools -- enter/exit plan-only mode where tool calls are recorded but not executed.
//!
//! When plan mode is active, the agent loop collects tool invocations in a PlanBuffer
//! instead of executing them. This allows the LLM to plan a sequence of operations
//! that can be reviewed before execution.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use super::traits::Tool;
use octo_types::{ToolContext, ToolOutput, ToolSource};

/// A recorded tool call in plan mode.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedAction {
    /// Sequential index (1-based).
    pub index: usize,
    /// Tool name.
    pub tool_name: String,
    /// Tool input parameters.
    pub input: Value,
    /// Brief description of what this action would do.
    pub description: String,
}

/// Shared buffer for collecting planned actions.
#[derive(Debug, Clone)]
pub struct PlanBuffer {
    inner: Arc<Mutex<PlanBufferInner>>,
}

#[derive(Debug, Default)]
struct PlanBufferInner {
    active: bool,
    actions: Vec<PlannedAction>,
}

impl Default for PlanBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlanBufferInner::default())),
        }
    }

    /// Enter plan mode.
    pub fn enter(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.active = true;
        inner.actions.clear();
    }

    /// Exit plan mode, returning all collected actions.
    pub fn exit(&self) -> Vec<PlannedAction> {
        let mut inner = self.inner.lock().unwrap();
        inner.active = false;
        std::mem::take(&mut inner.actions)
    }

    /// Whether plan mode is currently active.
    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().active
    }

    /// Record a planned action. Returns a simulated success response.
    pub fn record(&self, tool_name: &str, input: &Value) -> Option<String> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.active {
            return None;
        }

        let index = inner.actions.len() + 1;
        let description = summarize_tool_call(tool_name, input);

        inner.actions.push(PlannedAction {
            index,
            tool_name: tool_name.to_string(),
            input: input.clone(),
            description: description.clone(),
        });

        Some(format!(
            "[Plan mode] Recorded action #{index}: {description}. No changes made."
        ))
    }

    /// Get current plan summary.
    pub fn summary(&self) -> Vec<PlannedAction> {
        self.inner.lock().unwrap().actions.clone()
    }

    /// Number of recorded actions.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Generate a brief description of a tool call.
fn summarize_tool_call(tool_name: &str, input: &Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let truncated: String = cmd.chars().take(80).collect();
            format!("bash: {truncated}")
        }
        "file_read" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("read: {path}")
        }
        "file_write" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("write: {path}")
        }
        "file_edit" => {
            let path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("edit: {path}")
        }
        "grep" => {
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("grep: {pattern}")
        }
        "glob" => {
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("glob: {pattern}")
        }
        _ => {
            let serialized = serde_json::to_string(input).unwrap_or_default();
            let truncated: String = serialized.chars().take(100).collect();
            format!("{tool_name}: {truncated}")
        }
    }
}

// === Tools ===

/// Tool to enter plan mode.
pub struct EnterPlanModeTool {
    buffer: PlanBuffer,
}

impl EnterPlanModeTool {
    pub fn new(buffer: PlanBuffer) -> Self {
        Self { buffer }
    }
}

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Enter plan mode. While in plan mode, tool calls are recorded but NOT executed. \
         This lets you plan a sequence of operations without making any changes. \
         Use exit_plan_mode to review the plan and optionally execute it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Why you're entering plan mode (e.g., 'planning refactoring steps')"
                }
            }
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let reason = params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("planning");
        self.buffer.enter();
        Ok(ToolOutput::success(format!(
            "Plan mode activated (reason: {reason}). \
             All subsequent tool calls will be recorded but not executed. \
             Use exit_plan_mode to review and optionally execute the plan."
        )))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "planning"
    }
}

/// Tool to exit plan mode and review the plan.
pub struct ExitPlanModeTool {
    buffer: PlanBuffer,
}

impl ExitPlanModeTool {
    pub fn new(buffer: PlanBuffer) -> Self {
        Self { buffer }
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }

    fn description(&self) -> &str {
        "Exit plan mode and review the recorded plan. \
         Returns a summary of all planned actions. \
         The plan is NOT automatically executed -- you must explicitly execute each step."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let actions = self.buffer.exit();

        if actions.is_empty() {
            return Ok(ToolOutput::success(
                "Plan mode deactivated. No actions were recorded.",
            ));
        }

        let mut summary = format!(
            "Plan mode deactivated. {} actions recorded:\n\n",
            actions.len()
        );
        for action in &actions {
            summary.push_str(&format!(
                "{}. [{}] {}\n",
                action.index, action.tool_name, action.description
            ));
        }
        summary.push_str("\nTo execute, run each action individually in order.");

        Ok(ToolOutput::success(summary))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "planning"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_buffer_default() {
        let buf = PlanBuffer::new();
        assert!(!buf.is_active());
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_enter_exit() {
        let buf = PlanBuffer::new();
        buf.enter();
        assert!(buf.is_active());
        let actions = buf.exit();
        assert!(!buf.is_active());
        assert!(actions.is_empty());
    }

    #[test]
    fn test_record_when_active() {
        let buf = PlanBuffer::new();
        buf.enter();
        let result = buf.record("bash", &json!({"command": "ls -la"}));
        assert!(result.is_some());
        assert!(result.unwrap().contains("action #1"));
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn test_record_when_inactive() {
        let buf = PlanBuffer::new();
        let result = buf.record("bash", &json!({"command": "ls"}));
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_records() {
        let buf = PlanBuffer::new();
        buf.enter();
        buf.record("bash", &json!({"command": "cargo check"}));
        buf.record("file_edit", &json!({"file_path": "/src/main.rs"}));
        buf.record("bash", &json!({"command": "cargo test"}));
        assert_eq!(buf.len(), 3);
        let actions = buf.exit();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].index, 1);
        assert_eq!(actions[1].index, 2);
        assert_eq!(actions[2].index, 3);
    }

    #[test]
    fn test_enter_clears_previous() {
        let buf = PlanBuffer::new();
        buf.enter();
        buf.record("bash", &json!({"command": "ls"}));
        buf.enter(); // re-enter clears
        assert!(buf.is_empty());
    }

    #[test]
    fn test_summary() {
        let buf = PlanBuffer::new();
        buf.enter();
        buf.record("bash", &json!({"command": "cargo build"}));
        let summary = buf.summary();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].tool_name, "bash");
    }

    #[test]
    fn test_summarize_tool_call() {
        assert!(summarize_tool_call("bash", &json!({"command": "ls -la"})).contains("ls -la"));
        assert!(
            summarize_tool_call("file_read", &json!({"file_path": "/tmp/x"})).contains("/tmp/x")
        );
        assert!(
            summarize_tool_call("file_write", &json!({"file_path": "/tmp/y"})).contains("/tmp/y")
        );
        assert!(
            summarize_tool_call("file_edit", &json!({"file_path": "/tmp/z"})).contains("/tmp/z")
        );
        assert!(summarize_tool_call("grep", &json!({"pattern": "TODO"})).contains("TODO"));
        assert!(summarize_tool_call("glob", &json!({"pattern": "*.rs"})).contains("*.rs"));
        assert!(summarize_tool_call("unknown", &json!({"key": "val"})).contains("unknown"));
    }

    #[tokio::test]
    async fn test_enter_plan_mode_tool() {
        let buf = PlanBuffer::new();
        let tool = EnterPlanModeTool::new(buf.clone());
        let ctx = ToolContext {
            sandbox_id: Default::default(),
            working_dir: "/tmp".into(),
            path_validator: None,
        };
        let result = tool
            .execute(json!({"reason": "testing"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Plan mode activated"));
        assert!(buf.is_active());
    }

    #[tokio::test]
    async fn test_exit_plan_mode_tool_empty() {
        let buf = PlanBuffer::new();
        buf.enter();
        let tool = ExitPlanModeTool::new(buf.clone());
        let ctx = ToolContext {
            sandbox_id: Default::default(),
            working_dir: "/tmp".into(),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("No actions were recorded"));
    }

    #[tokio::test]
    async fn test_exit_plan_mode_tool_with_actions() {
        let buf = PlanBuffer::new();
        buf.enter();
        buf.record("bash", &json!({"command": "cargo check"}));
        buf.record("file_edit", &json!({"file_path": "/src/lib.rs"}));
        let tool = ExitPlanModeTool::new(buf.clone());
        let ctx = ToolContext {
            sandbox_id: Default::default(),
            working_dir: "/tmp".into(),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await.unwrap();
        assert!(result.content.contains("2 actions recorded"));
        assert!(result.content.contains("cargo check"));
        assert!(result.content.contains("/src/lib.rs"));
        assert!(!buf.is_active());
    }

    #[test]
    fn test_tool_metadata() {
        let buf = PlanBuffer::new();
        let enter = EnterPlanModeTool::new(buf.clone());
        let exit = ExitPlanModeTool::new(buf);
        assert_eq!(enter.name(), "enter_plan_mode");
        assert_eq!(exit.name(), "exit_plan_mode");
        assert!(enter.is_read_only());
        assert!(exit.is_read_only());
        assert_eq!(enter.category(), "planning");
        assert_eq!(exit.category(), "planning");
    }
}
