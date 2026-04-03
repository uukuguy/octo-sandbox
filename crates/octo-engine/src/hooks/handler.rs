use super::context::HookContext;
use async_trait::async_trait;
use std::fmt;

/// Hook failure mode — determines behavior when a hook handler returns an error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookFailureMode {
    /// Hook errors are non-fatal: log a warning and continue (default)
    FailOpen,
    /// Hook errors are fatal: abort the operation (for security-critical hooks)
    FailClosed,
}

/// Hook-returned permission decision (AP-T11).
#[derive(Debug, Clone)]
pub enum PermissionHookDecision {
    /// Allow tool execution
    Allow,
    /// Deny execution with reason
    Deny(String),
    /// Require human confirmation
    Ask,
}

/// Action returned by a hook handler
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum HookAction {
    /// Continue processing (no modification)
    Continue,
    /// Continue with modified context
    Modify(HookContext),
    /// Abort the operation with a reason
    Abort(String),
    /// Soft-deny: block with a reason, caller decides how to handle
    Block(String),
    /// Redirect to another agent or tool
    Redirect(String),
    /// Modify the tool input parameters (PreToolUse only)
    ModifyInput(serde_json::Value),
    /// Inject additional context into the next LLM call as `<system-reminder>`
    InjectContext(String),
    /// Override the permission decision for this tool call (PreToolUse only)
    PermissionOverride(PermissionHookDecision),
}

/// Trait for hook handlers
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// Human-readable name
    fn name(&self) -> &str;

    /// Priority (lower = runs first, default 100)
    fn priority(&self) -> u32 {
        100
    }

    /// Failure mode: FailOpen (default) or FailClosed
    fn failure_mode(&self) -> HookFailureMode {
        HookFailureMode::FailOpen
    }

    /// Whether this hook executes asynchronously (fire-and-forget, not blocking the main loop).
    fn is_async(&self) -> bool {
        false
    }

    /// Execute the hook
    async fn execute(&self, context: &HookContext) -> anyhow::Result<HookAction>;
}

/// Type alias for boxed hook handlers
pub type BoxHookHandler = Box<dyn HookHandler>;

impl fmt::Debug for dyn HookHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HookHandler({})", self.name())
    }
}
