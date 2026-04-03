use super::context::HookContext;
use super::handler::HookFailureMode;
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

fn validate_redirect_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 128
        // Restrict to ASCII alphanumeric + underscore + hyphen only.
        // is_ascii_alphanumeric() excludes Unicode letters (Cyrillic, CJK, etc.)
        // which could be used to bypass keyword filters.
        && target.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
            // P2-H4: Async hooks — fire-and-forget, don't block the main loop
            if handler.is_async() {
                let ctx = current_context.clone();
                let h = Arc::clone(handler);
                tokio::spawn(async move {
                    if let Err(e) = h.execute(&ctx).await {
                        warn!(hook = h.name(), "Async hook error: {e}");
                    }
                });
                continue;
            }

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
                // New AP-T11 actions: pass through immediately to caller
                Ok(action @ HookAction::ModifyInput(_)) => {
                    debug!(
                        hook_point = ?point,
                        handler = handler.name(),
                        "Hook modified tool input"
                    );
                    return action;
                }
                Ok(action @ HookAction::InjectContext(_)) => {
                    debug!(
                        hook_point = ?point,
                        handler = handler.name(),
                        "Hook injecting context"
                    );
                    return action;
                }
                Ok(action @ HookAction::PermissionOverride(_)) => {
                    debug!(
                        hook_point = ?point,
                        handler = handler.name(),
                        "Hook overriding permission"
                    );
                    return action;
                }
                Ok(HookAction::Redirect(target)) => {
                    if !validate_redirect_target(&target) {
                        warn!(
                            hook_point = ?point,
                            handler = handler.name(),
                            target = %target,
                            "HookRegistry: invalid Redirect target, treating as Continue"
                        );
                        return HookAction::Continue;
                    }
                    debug!(
                        hook_point = ?point,
                        handler = handler.name(),
                        target = %target,
                        "Hook redirected"
                    );
                    return HookAction::Redirect(target);
                }
                Err(e) => {
                    match handler.failure_mode() {
                        HookFailureMode::FailOpen => {
                            // Hook errors are non-fatal -- log and continue
                            warn!(
                                hook_point = ?point,
                                handler = handler.name(),
                                error = %e,
                                "Hook handler error (FailOpen), continuing"
                            );
                        }
                        HookFailureMode::FailClosed => {
                            warn!(
                                hook_point = ?point,
                                handler = handler.name(),
                                error = %e,
                                "Hook handler error (FailClosed), aborting"
                            );
                            return HookAction::Abort(format!(
                                "FailClosed hook '{}' error: {}",
                                handler.name(),
                                e
                            ));
                        }
                    }
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
        handlers.get(&point).is_some_and(|h| !h.is_empty())
    }

    /// Get count of registered handlers for a hook point
    pub async fn handler_count(&self, point: HookPoint) -> usize {
        let handlers = self.handlers.read().await;
        handlers.get(&point).map_or(0, |h| h.len())
    }

    /// Create a scoped copy of this registry, keeping only handlers whose
    /// names match any of the provided scope tags (AY-D5).
    ///
    /// The returned registry is independent and does not share state with the original.
    pub async fn scoped(&self, scope: &[String]) -> Self {
        let handlers = self.handlers.read().await;
        let mut scoped_handlers: HashMap<HookPoint, Vec<Arc<dyn HookHandler>>> = HashMap::new();

        for (point, handler_list) in handlers.iter() {
            let filtered: Vec<Arc<dyn HookHandler>> = handler_list
                .iter()
                .filter(|h| scope.iter().any(|s| h.name().contains(s.as_str())))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                scoped_handlers.insert(*point, filtered);
            }
        }

        Self {
            handlers: RwLock::new(scoped_handlers),
        }
    }

    /// List all registered hook points with their handler counts
    pub async fn list_all(&self) -> Vec<(HookPoint, usize)> {
        let handlers = self.handlers.read().await;
        handlers
            .iter()
            .map(|(point, handlers)| (*point, handlers.len()))
            .collect()
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

    #[tokio::test]
    async fn test_scoped_filters_by_name() {
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::PreToolUse, Arc::new(BlockingHandler))
            .await;
        registry
            .register(HookPoint::PreToolUse, Arc::new(ContinueHandler))
            .await;

        // Scope to only "blocker"
        let scoped = registry.scoped(&["blocker".to_string()]).await;
        assert_eq!(scoped.handler_count(HookPoint::PreToolUse).await, 1);
    }

    #[tokio::test]
    async fn test_scoped_empty_scope_filters_all() {
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::PreToolUse, Arc::new(BlockingHandler))
            .await;

        let scoped = registry.scoped(&[]).await;
        assert_eq!(scoped.handler_count(HookPoint::PreToolUse).await, 0);
    }

    // validate_redirect_target() tests
    #[test]
    fn test_redirect_target_valid() {
        assert!(validate_redirect_target("agent-b"));
        assert!(validate_redirect_target("my_agent_1"));
        assert!(validate_redirect_target("A"));
        assert!(validate_redirect_target(&"x".repeat(128)));
    }

    #[test]
    fn test_redirect_target_rejects_empty() {
        assert!(!validate_redirect_target(""));
    }

    #[test]
    fn test_redirect_target_rejects_too_long() {
        assert!(!validate_redirect_target(&"a".repeat(129)));
    }

    #[test]
    fn test_redirect_target_rejects_special_chars() {
        assert!(
            !validate_redirect_target("agent/../etc/passwd"),
            "path traversal rejected"
        );
        assert!(!validate_redirect_target("agent b"), "space rejected");
        assert!(!validate_redirect_target("agent@host"), "@ rejected");
        assert!(!validate_redirect_target("http://evil.com"), "URL rejected");
        assert!(
            !validate_redirect_target("agent\x00name"),
            "null byte rejected"
        );
    }

    #[test]
    fn test_redirect_target_rejects_unicode() {
        assert!(!validate_redirect_target("агент"), "Cyrillic rejected");
        assert!(!validate_redirect_target("代理"), "CJK rejected");
    }

    #[tokio::test]
    async fn test_invalid_redirect_target_falls_through_to_continue() {
        // When a handler returns Redirect with an invalid target the registry
        // should log a warning and return Continue instead of passing it on.
        struct BadRedirectHandler;
        #[async_trait]
        impl HookHandler for BadRedirectHandler {
            fn name(&self) -> &str {
                "bad-redirect"
            }
            async fn execute(&self, _ctx: &HookContext) -> anyhow::Result<HookAction> {
                Ok(HookAction::Redirect("../../etc/passwd".to_string()))
            }
        }
        let registry = HookRegistry::new();
        registry
            .register(HookPoint::AgentRoute, Arc::new(BadRedirectHandler))
            .await;
        let ctx = HookContext::new();
        let action = registry.execute(HookPoint::AgentRoute, &ctx).await;
        assert!(
            matches!(action, HookAction::Continue),
            "invalid redirect target must fall through to Continue, got {:?}",
            action
        );
    }
}
