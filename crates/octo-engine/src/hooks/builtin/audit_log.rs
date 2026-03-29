//! AuditLogHandler — PostToolUse hook that emits structured audit log entries.
//!
//! Records tool execution metadata (session, tool name, success, duration) via
//! `tracing::info!`. This provides a structured audit trail that can be
//! collected by log aggregators for compliance reporting.

use async_trait::async_trait;

use crate::hooks::{HookAction, HookContext, HookFailureMode, HookHandler};

/// PostToolUse hook handler that logs tool execution summaries.
///
/// Always returns `Continue` — audit logging never blocks execution.
/// Uses `FailOpen` so logging errors don't affect the agent loop.
pub struct AuditLogHandler;

#[async_trait]
impl HookHandler for AuditLogHandler {
    fn name(&self) -> &str {
        "audit-log"
    }

    fn priority(&self) -> u32 {
        200 // Low priority — runs after security checks
    }

    fn failure_mode(&self) -> HookFailureMode {
        HookFailureMode::FailOpen
    }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        tracing::info!(
            hook = "audit",
            session_id = ctx.session_id.as_deref().unwrap_or("unknown"),
            tool = ctx.tool_name.as_deref().unwrap_or("unknown"),
            success = ctx.success.unwrap_or(true),
            duration_ms = ctx.duration_ms.unwrap_or(0),
            turn = ctx.turn.unwrap_or(0),
            sandbox_mode = ctx.sandbox_mode.as_deref().unwrap_or("unknown"),
            "Tool execution audit"
        );
        Ok(HookAction::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_audit_log_continues() {
        let handler = AuditLogHandler;
        let ctx = HookContext::new()
            .with_session("s1")
            .with_tool("bash", json!({"command": "ls"}))
            .with_result(true, 42);
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_audit_log_with_failure() {
        let handler = AuditLogHandler;
        let ctx = HookContext::new()
            .with_session("s2")
            .with_tool("file_write", json!({"path": "/tmp/x"}))
            .with_result(false, 100);
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[tokio::test]
    async fn test_audit_log_minimal_context() {
        let handler = AuditLogHandler;
        let ctx = HookContext::new();
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }

    #[test]
    fn test_audit_handler_metadata() {
        let handler = AuditLogHandler;
        assert_eq!(handler.name(), "audit-log");
        assert_eq!(handler.priority(), 200);
        assert_eq!(handler.failure_mode(), HookFailureMode::FailOpen);
    }
}
