//! Collaboration handle — wraps an `AgentExecutorHandle` with collaboration capabilities.
//!
//! This is a non-invasive extension: the original `AgentExecutorHandle` is untouched,
//! and collaboration features are layered on top via composition.

use std::sync::Arc;

use crate::agent::executor::AgentExecutorHandle;

use super::context::{CollaborationContext, CollaborationEvent};

/// Extended handle that wraps an `AgentExecutorHandle` with collaboration capabilities.
///
/// Instead of modifying the widely-used `AgentExecutorHandle`, this wrapper adds
/// shared-state access, event logging, and context retrieval for multi-agent sessions.
pub struct CollaborationHandle {
    /// The base executor handle.
    pub executor: AgentExecutorHandle,
    /// Agent's ID in the collaboration.
    pub agent_id: String,
    /// Shared collaboration context.
    pub context: Arc<CollaborationContext>,
}

impl CollaborationHandle {
    /// Creates a new collaboration handle wrapping the given executor handle.
    pub fn new(
        executor: AgentExecutorHandle,
        agent_id: String,
        context: Arc<CollaborationContext>,
    ) -> Self {
        Self {
            executor,
            agent_id,
            context,
        }
    }

    /// Returns the agent's ID in this collaboration.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Reads a value from the shared collaboration state.
    pub fn get_shared_state(&self, key: &str) -> Option<serde_json::Value> {
        self.context.get_state(key)
    }

    /// Writes a value into the shared collaboration state and logs a `StateUpdated` event.
    pub fn set_shared_state(&self, key: String, value: serde_json::Value) {
        self.context.set_state(key.clone(), value);
        self.context.log_event(CollaborationEvent::StateUpdated {
            agent_id: self.agent_id.clone(),
            key,
        });
    }

    /// Returns a reference to the underlying collaboration context.
    pub fn context(&self) -> &Arc<CollaborationContext> {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::events::AgentEvent;
    use octo_types::SessionId;
    use tokio::sync::{broadcast, mpsc};

    fn make_executor_handle() -> AgentExecutorHandle {
        let (tx, _rx) = mpsc::channel(8);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(8);
        AgentExecutorHandle {
            tx,
            broadcast_tx,
            session_id: SessionId::default(),
        }
    }

    fn make_collaboration_handle() -> CollaborationHandle {
        let executor = make_executor_handle();
        let context = Arc::new(CollaborationContext::new("test-collab".to_string()));
        CollaborationHandle::new(executor, "agent-1".to_string(), context)
    }

    #[test]
    fn creation_and_agent_id() {
        let handle = make_collaboration_handle();
        assert_eq!(handle.agent_id(), "agent-1");
    }

    #[test]
    fn get_set_shared_state() {
        let handle = make_collaboration_handle();

        // Initially empty
        assert!(handle.get_shared_state("key1").is_none());

        // Set and read back
        handle.set_shared_state("key1".to_string(), serde_json::json!("hello"));
        assert_eq!(
            handle.get_shared_state("key1"),
            Some(serde_json::json!("hello"))
        );

        // Overwrite
        handle.set_shared_state("key1".to_string(), serde_json::json!(42));
        assert_eq!(
            handle.get_shared_state("key1"),
            Some(serde_json::json!(42))
        );
    }

    #[test]
    fn set_shared_state_logs_event() {
        let handle = make_collaboration_handle();
        handle.set_shared_state("progress".to_string(), serde_json::json!(0.5));

        let events = handle.context.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            CollaborationEvent::StateUpdated { agent_id, key } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(key, "progress");
            }
            other => panic!("Expected StateUpdated, got {:?}", other),
        }
    }

    #[test]
    fn context_accessor() {
        let handle = make_collaboration_handle();
        assert_eq!(handle.context().id, "test-collab");
    }

    #[test]
    fn shared_state_visible_across_handles() {
        let context = Arc::new(CollaborationContext::new("shared".to_string()));
        let executor1 = make_executor_handle();
        let executor2 = make_executor_handle();

        let h1 = CollaborationHandle::new(executor1, "a1".to_string(), Arc::clone(&context));
        let h2 = CollaborationHandle::new(executor2, "a2".to_string(), Arc::clone(&context));

        h1.set_shared_state("result".to_string(), serde_json::json!("done"));
        assert_eq!(
            h2.get_shared_state("result"),
            Some(serde_json::json!("done"))
        );
    }
}
