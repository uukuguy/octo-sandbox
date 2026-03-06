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
