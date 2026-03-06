use super::context::HookContext;
use async_trait::async_trait;
use std::fmt;

/// Action returned by a hook handler
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Continue processing (no modification)
    Continue,
    /// Continue with modified context
    Modify(HookContext),
    /// Abort the operation with a reason
    Abort(String),
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
