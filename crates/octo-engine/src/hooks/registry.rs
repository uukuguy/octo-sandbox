use super::context::HookContext;
use super::{HookAction, HookHandler, HookPoint};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Registry managing hook handlers for all hook points
#[derive(Default)]
pub struct HookRegistry {
    handlers: RwLock<HashMap<HookPoint, Vec<Arc<dyn HookHandler>>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a hook handler for a specific hook point
    pub async fn register(&self, point: HookPoint, handler: Arc<dyn HookHandler>) {
        let mut handlers = self.handlers.write().await;
        let entry = handlers.entry(point).or_default();
        let handler_name = handler.name().to_owned();
        entry.push(handler);
        // Sort by priority (lower = first)
        entry.sort_by_key(|h| h.priority());
        debug!(hook_point = ?point, handler = %handler_name, "Hook handler registered");
    }

    /// Execute all handlers for a hook point in priority order.
    /// Returns the final HookAction (Continue, Modify, or Abort).
    /// If any handler returns Abort, execution stops immediately.
    pub async fn execute(&self, point: HookPoint, context: &HookContext) -> HookAction {
        let handlers = self.handlers.read().await;
        let Some(handlers) = handlers.get(&point) else {
            return HookAction::Continue;
        };

        let mut current_context = context.clone();

        for handler in handlers {
            match handler.execute(&current_context).await {
                Ok(HookAction::Continue) => {
                    // Continue to next handler
                }
                Ok(HookAction::Modify(new_ctx)) => {
                    current_context = new_ctx;
                }
                Ok(HookAction::Abort(reason)) => {
                    warn!(
                        hook_point = ?point,
                        handler = handler.name(),
                        reason = %reason,
                        "Hook aborted operation"
                    );
                    return HookAction::Abort(reason);
                }
                Ok(HookAction::Block(reason)) => {
                    warn!(
                        hook_point = ?point,
                        handler = handler.name(),
                        reason = %reason,
                        "Hook blocked operation (soft-deny)"
                    );
                    return HookAction::Block(reason);
                }
                Ok(HookAction::Redirect(target)) => {
                    debug!(
                        hook_point = ?point,
                        handler = handler.name(),
                        target = %target,
                        "Hook redirected"
                    );
                    return HookAction::Redirect(target);
                }
                Err(e) => {
                    // Hook errors are non-fatal -- log and continue
                    warn!(
                        hook_point = ?point,
                        handler = handler.name(),
                        error = %e,
                        "Hook handler error, continuing"
                    );
                }
            }
        }

        if current_context.metadata != context.metadata
            || current_context.tool_input != context.tool_input
        {
            HookAction::Modify(current_context)
        } else {
            HookAction::Continue
        }
    }

    /// Check if any handlers are registered for a hook point
    pub async fn has_handlers(&self, point: HookPoint) -> bool {
        let handlers = self.handlers.read().await;
        handlers.get(&point).map_or(false, |h| !h.is_empty())
    }

    /// Get count of registered handlers for a hook point
    pub async fn handler_count(&self, point: HookPoint) -> usize {
        let handlers = self.handlers.read().await;
        handlers.get(&point).map_or(0, |h| h.len())
    }
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HookRegistry {{ ... }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct BlockingHandler;
    #[async_trait]
    impl HookHandler for BlockingHandler {
        fn name(&self) -> &str {
            "blocker"
        }
        async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
            Ok(HookAction::Block("test-block".to_string()))
        }
    }

    struct RedirectingHandler;
    #[async_trait]
    impl HookHandler for RedirectingHandler {
        fn name(&self) -> &str {
            "redirector"
        }
        async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
            Ok(HookAction::Redirect("agent-b".to_string()))
        }
    }

    struct ContinueHandler;
    #[async_trait]
    impl HookHandler for ContinueHandler {
        fn name(&self) -> &str {
            "continuer"
        }
        async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
            Ok(HookAction::Continue)
        }
    }

    #[tokio::test]
    async fn test_block_action_stops_chain() {
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::PreToolUse, Arc::new(BlockingHandler))
            .await;
        let ctx = HookContext::new().with_session("s1");
        let action = registry.execute(HookPoint::PreToolUse, &ctx).await;
        assert!(
            matches!(action, HookAction::Block(ref r) if r == "test-block"),
            "expected Block(test-block), got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn test_redirect_action() {
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::AgentRoute, Arc::new(RedirectingHandler))
            .await;
        let ctx = HookContext::new();
        let action = registry.execute(HookPoint::AgentRoute, &ctx).await;
        assert!(
            matches!(action, HookAction::Redirect(ref t) if t == "agent-b"),
            "expected Redirect(agent-b), got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn test_no_handlers_returns_continue() {
        let registry = HookRegistry::new();
        let ctx = HookContext::new();
        let action = registry.execute(HookPoint::SessionStart, &ctx).await;
        assert!(
            matches!(action, HookAction::Continue),
            "expected Continue, got {:?}",
            action
        );
    }

    #[tokio::test]
    async fn test_continue_handler_returns_continue() {
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::PostTask, Arc::new(ContinueHandler))
            .await;
        let ctx = HookContext::new().with_session("s2");
        let action = registry.execute(HookPoint::PostTask, &ctx).await;
        assert!(
            matches!(action, HookAction::Continue),
            "expected Continue, got {:?}",
            action
        );
    }
}
