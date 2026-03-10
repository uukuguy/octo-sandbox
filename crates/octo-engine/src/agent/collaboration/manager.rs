//! CollaborationManager — manages N agents in a collaboration session.
//!
//! Provides agent registration, active-agent switching, inter-agent channels,
//! and a shared [`CollaborationContext`] for state, events, and proposals.

use std::collections::HashMap;
use std::sync::Arc;

use octo_types::SessionId;

use crate::agent::capability::AgentCapability;
use crate::agent::executor::AgentExecutorHandle;

use super::channel::{create_channel_pair, CollaborationChannel};
use super::context::{CollaborationContext, CollaborationEvent, CollaborationStatus};

/// Agent info stored in the collaboration.
#[derive(Clone)]
pub struct CollaborationAgent {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<AgentCapability>,
    pub handle: AgentExecutorHandle,
}

impl std::fmt::Debug for CollaborationAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollaborationAgent")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("capabilities", &self.capabilities)
            .field("session_id", &self.handle.session_id)
            .finish()
    }
}

/// Manages N agents collaborating in a shared session.
pub struct CollaborationManager {
    /// Participating agents (by agent_id).
    agents: HashMap<String, CollaborationAgent>,
    /// Currently active agent (REPL focus).
    active_agent: String,
    /// Shared collaboration context.
    context: Arc<CollaborationContext>,
    /// Communication channels between agent pairs.
    /// Key is (from_agent, to_agent) for the directed half.
    channels: HashMap<(String, String), CollaborationChannel>,
    /// Session ID shared by all agents.
    session_id: SessionId,
}

impl CollaborationManager {
    /// Creates an empty manager with a fresh context.
    pub fn new(session_id: SessionId) -> Self {
        let context = Arc::new(CollaborationContext::new(format!(
            "collab-{}",
            session_id
        )));
        Self {
            agents: HashMap::new(),
            active_agent: String::new(),
            context,
            channels: HashMap::new(),
            session_id,
        }
    }

    /// Creates an empty manager with a pre-existing context.
    pub fn with_context(session_id: SessionId, context: Arc<CollaborationContext>) -> Self {
        Self {
            agents: HashMap::new(),
            active_agent: String::new(),
            context,
            channels: HashMap::new(),
            session_id,
        }
    }

    /// Creates a manager pre-configured with two agents (plan + build),
    /// mirroring the old `DualAgentManager` setup.
    pub fn dual_mode(
        plan_id: String,
        plan_name: String,
        plan_handle: AgentExecutorHandle,
        build_id: String,
        build_name: String,
        build_handle: AgentExecutorHandle,
        session_id: SessionId,
    ) -> Self {
        let mut mgr = Self::new(session_id);
        mgr.add_agent(plan_id.clone(), plan_name, vec![], plan_handle);
        mgr.add_agent(build_id, build_name, vec![], build_handle);
        // Default active agent is the first one added (plan).
        mgr.active_agent = plan_id;
        mgr
    }

    /// Adds an agent to the collaboration.
    ///
    /// Creates bidirectional channels to every existing agent and logs
    /// an `AgentJoined` event to the shared context.
    pub fn add_agent(
        &mut self,
        id: String,
        name: String,
        capabilities: Vec<AgentCapability>,
        handle: AgentExecutorHandle,
    ) {
        // Create channel pairs to all existing agents.
        let existing_ids: Vec<String> = self.agents.keys().cloned().collect();
        for existing_id in &existing_ids {
            let (ch_new_to_existing, ch_existing_to_new) =
                create_channel_pair(&id, existing_id, 32);
            self.channels
                .insert((id.clone(), existing_id.clone()), ch_new_to_existing);
            self.channels
                .insert((existing_id.clone(), id.clone()), ch_existing_to_new);
        }

        // Log the join event.
        self.context
            .log_event(CollaborationEvent::AgentJoined {
                agent_id: id.clone(),
                capabilities: capabilities.clone(),
            });

        // If this is the first agent, make it active by default.
        if self.agents.is_empty() {
            self.active_agent = id.clone();
        }

        self.agents.insert(
            id.clone(),
            CollaborationAgent {
                id,
                name,
                capabilities,
                handle,
            },
        );
    }

    /// Removes an agent from the collaboration.
    ///
    /// Removes all channels involving this agent and logs an `AgentLeft` event.
    /// Returns the removed agent info, or `None` if not found.
    pub fn remove_agent(&mut self, agent_id: &str) -> Option<CollaborationAgent> {
        let agent = self.agents.remove(agent_id)?;

        // Remove all channels involving this agent.
        self.channels
            .retain(|(from, to), _| from != agent_id && to != agent_id);

        // Log the leave event.
        self.context
            .log_event(CollaborationEvent::AgentLeft {
                agent_id: agent_id.to_string(),
            });

        // If the removed agent was active, switch to another if available.
        if self.active_agent == agent_id {
            self.active_agent = self
                .agents
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
        }

        Some(agent)
    }

    /// Returns the ID of the currently active agent.
    pub fn active_agent_id(&self) -> &str {
        &self.active_agent
    }

    /// Switches the active agent to the given ID.
    /// Returns `false` if the agent is not in the collaboration.
    pub fn switch_to(&mut self, agent_id: &str) -> bool {
        if self.agents.contains_key(agent_id) {
            self.active_agent = agent_id.to_string();
            true
        } else {
            false
        }
    }

    /// Returns a reference to the active agent's executor handle.
    pub fn active_handle(&self) -> Option<&AgentExecutorHandle> {
        self.agents.get(&self.active_agent).map(|a| &a.handle)
    }

    /// Returns a reference to a specific agent's executor handle.
    pub fn get_handle(&self, agent_id: &str) -> Option<&AgentExecutorHandle> {
        self.agents.get(agent_id).map(|a| &a.handle)
    }

    /// Returns the IDs of all participating agents.
    pub fn agent_ids(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }

    /// Returns the number of participating agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Returns a reference to the shared collaboration context.
    pub fn context(&self) -> &Arc<CollaborationContext> {
        &self.context
    }

    /// Returns a reference to the shared session ID.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Builds a status snapshot of this collaboration.
    pub fn status(&self) -> CollaborationStatus {
        self.context.status(
            self.agents.len(),
            if self.active_agent.is_empty() {
                None
            } else {
                Some(self.active_agent.clone())
            },
        )
    }

    /// Returns a mutable reference to the channel from `from` to `to`.
    pub fn get_channel_mut(
        &mut self,
        from: &str,
        to: &str,
    ) -> Option<&mut CollaborationChannel> {
        self.channels
            .get_mut(&(from.to_string(), to.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{broadcast, mpsc};

    fn make_test_handle(session_name: &str) -> AgentExecutorHandle {
        let (tx, _rx) = mpsc::channel(1);
        let (btx, _) = broadcast::channel(1);
        AgentExecutorHandle {
            tx,
            broadcast_tx: btx,
            session_id: SessionId::from_string(session_name),
        }
    }

    #[test]
    fn new_creates_empty_manager() {
        let sid = SessionId::from_string("s1");
        let mgr = CollaborationManager::new(sid);
        assert_eq!(mgr.agent_count(), 0);
        assert!(mgr.active_agent_id().is_empty());
        assert!(mgr.active_handle().is_none());
    }

    #[test]
    fn add_agent_and_count() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent(
            "a1".into(),
            "Agent One".into(),
            vec![AgentCapability::CodeGeneration],
            make_test_handle("s1"),
        );
        assert_eq!(mgr.agent_count(), 1);

        mgr.add_agent(
            "a2".into(),
            "Agent Two".into(),
            vec![AgentCapability::Testing],
            make_test_handle("s1"),
        );
        assert_eq!(mgr.agent_count(), 2);

        // First agent becomes active by default.
        assert_eq!(mgr.active_agent_id(), "a1");
    }

    #[test]
    fn remove_agent() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));

        let removed = mgr.remove_agent("a1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "a1");
        assert_eq!(mgr.agent_count(), 1);

        // Active agent should have switched.
        assert_eq!(mgr.active_agent_id(), "a2");

        // Removing non-existent returns None.
        assert!(mgr.remove_agent("a999").is_none());
    }

    #[test]
    fn switch_to_active_agent() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));

        assert_eq!(mgr.active_agent_id(), "a1");

        assert!(mgr.switch_to("a2"));
        assert_eq!(mgr.active_agent_id(), "a2");

        // Switching to non-existent agent returns false.
        assert!(!mgr.switch_to("a999"));
        // Active agent unchanged.
        assert_eq!(mgr.active_agent_id(), "a2");
    }

    #[test]
    fn active_handle_returns_correct_handle() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        let handle_a1 = make_test_handle("session-a1");
        let handle_a2 = make_test_handle("session-a2");

        mgr.add_agent("a1".into(), "One".into(), vec![], handle_a1);
        mgr.add_agent("a2".into(), "Two".into(), vec![], handle_a2);

        // Active is a1.
        let h = mgr.active_handle().unwrap();
        assert_eq!(h.session_id, SessionId::from_string("session-a1"));

        mgr.switch_to("a2");
        let h = mgr.active_handle().unwrap();
        assert_eq!(h.session_id, SessionId::from_string("session-a2"));
    }

    #[test]
    fn dual_mode_convenience_constructor() {
        let sid = SessionId::from_string("dual-session");
        let mgr = CollaborationManager::dual_mode(
            "plan".into(),
            "Planner".into(),
            make_test_handle("dual-session"),
            "build".into(),
            "Builder".into(),
            make_test_handle("dual-session"),
            sid,
        );

        assert_eq!(mgr.agent_count(), 2);
        assert_eq!(mgr.active_agent_id(), "plan");

        let ids = mgr.agent_ids();
        assert!(ids.contains(&"plan".to_string()));
        assert!(ids.contains(&"build".to_string()));
    }

    #[test]
    fn status_reflects_current_state() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        let status = mgr.status();
        assert_eq!(status.agent_count, 0);
        assert!(status.active_agent.is_none());

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));

        let status = mgr.status();
        assert_eq!(status.agent_count, 2);
        assert_eq!(status.active_agent, Some("a1".to_string()));
        // Two AgentJoined events.
        assert_eq!(status.event_count, 2);
    }

    #[test]
    fn channels_created_between_agents() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));

        // Bidirectional channels should exist.
        assert!(mgr.get_channel_mut("a1", "a2").is_some());
        assert!(mgr.get_channel_mut("a2", "a1").is_some());

        // No self-channel.
        assert!(mgr.get_channel_mut("a1", "a1").is_none());
    }

    #[test]
    fn add_third_agent_creates_channels_to_both_existing() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a3".into(), "Three".into(), vec![], make_test_handle("s1"));

        // a3 should have channels to both a1 and a2.
        assert!(mgr.get_channel_mut("a3", "a1").is_some());
        assert!(mgr.get_channel_mut("a1", "a3").is_some());
        assert!(mgr.get_channel_mut("a3", "a2").is_some());
        assert!(mgr.get_channel_mut("a2", "a3").is_some());

        // Original a1 <-> a2 channels still exist.
        assert!(mgr.get_channel_mut("a1", "a2").is_some());
        assert!(mgr.get_channel_mut("a2", "a1").is_some());
    }

    #[test]
    fn get_handle_by_id() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent(
            "a1".into(),
            "One".into(),
            vec![],
            make_test_handle("handle-s1"),
        );

        let h = mgr.get_handle("a1").unwrap();
        assert_eq!(h.session_id, SessionId::from_string("handle-s1"));

        assert!(mgr.get_handle("missing").is_none());
    }

    #[test]
    fn remove_agent_cleans_up_channels() {
        let sid = SessionId::from_string("s1");
        let mut mgr = CollaborationManager::new(sid);

        mgr.add_agent("a1".into(), "One".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a2".into(), "Two".into(), vec![], make_test_handle("s1"));
        mgr.add_agent("a3".into(), "Three".into(), vec![], make_test_handle("s1"));

        // Before removal: 6 directed channels (3 pairs).
        assert!(mgr.get_channel_mut("a1", "a2").is_some());
        assert!(mgr.get_channel_mut("a2", "a3").is_some());

        mgr.remove_agent("a2");

        // All channels involving a2 should be gone.
        assert!(mgr.get_channel_mut("a1", "a2").is_none());
        assert!(mgr.get_channel_mut("a2", "a1").is_none());
        assert!(mgr.get_channel_mut("a2", "a3").is_none());
        assert!(mgr.get_channel_mut("a3", "a2").is_none());

        // a1 <-> a3 channels should remain.
        assert!(mgr.get_channel_mut("a1", "a3").is_some());
        assert!(mgr.get_channel_mut("a3", "a1").is_some());
    }
}
