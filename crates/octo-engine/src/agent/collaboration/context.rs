//! Collaboration context shared by participating agents.
//!
//! Provides shared state, event logging, and proposal-based decision making
//! for multi-agent collaboration sessions.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::agent::capability::AgentCapability;

/// Collaboration context shared by all participating agents.
///
/// Uses `RwLock` for interior mutability so that multiple agents can
/// concurrently read shared state while writes are serialized.
pub struct CollaborationContext {
    pub id: String,
    pub shared_state: RwLock<HashMap<String, serde_json::Value>>,
    pub log: RwLock<Vec<CollaborationEvent>>,
    pub proposals: RwLock<Vec<Proposal>>,
}

/// A proposal submitted by an agent for group decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub from_agent: String,
    pub action: String,
    pub description: String,
    pub status: ProposalStatus,
    pub votes: Vec<Vote>,
}

/// Status of a proposal in the voting lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
}

/// A vote cast by an agent on a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub agent_id: String,
    pub approve: bool,
    pub reason: Option<String>,
}

/// Events that occur during a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborationEvent {
    AgentJoined {
        agent_id: String,
        capabilities: Vec<AgentCapability>,
    },
    AgentLeft {
        agent_id: String,
    },
    MessageSent {
        from: String,
        to: String,
        content: String,
    },
    TaskDelegated {
        from: String,
        to: String,
        task: String,
    },
    StateUpdated {
        agent_id: String,
        key: String,
    },
}

/// Summary snapshot of a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationStatus {
    pub id: String,
    pub agent_count: usize,
    pub active_agent: Option<String>,
    pub pending_proposals: usize,
    pub event_count: usize,
    pub state_keys: Vec<String>,
}

impl CollaborationContext {
    /// Creates a new empty collaboration context.
    pub fn new(id: String) -> Self {
        Self {
            id,
            shared_state: RwLock::new(HashMap::new()),
            log: RwLock::new(Vec::new()),
            proposals: RwLock::new(Vec::new()),
        }
    }

    /// Reads a value from shared state by key.
    pub fn get_state(&self, key: &str) -> Option<serde_json::Value> {
        let state = self.shared_state.read().expect("shared_state lock poisoned");
        state.get(key).cloned()
    }

    /// Writes a value into shared state.
    pub fn set_state(&self, key: String, value: serde_json::Value) {
        let mut state = self.shared_state.write().expect("shared_state lock poisoned");
        state.insert(key, value);
    }

    /// Removes a key from shared state, returning its value if present.
    pub fn remove_state(&self, key: &str) -> Option<serde_json::Value> {
        let mut state = self.shared_state.write().expect("shared_state lock poisoned");
        state.remove(key)
    }

    /// Appends an event to the collaboration log.
    pub fn log_event(&self, event: CollaborationEvent) {
        let mut log = self.log.write().expect("log lock poisoned");
        log.push(event);
    }

    /// Returns a clone of all collaboration events.
    pub fn events(&self) -> Vec<CollaborationEvent> {
        let log = self.log.read().expect("log lock poisoned");
        log.clone()
    }

    /// Adds a proposal for group voting.
    pub fn add_proposal(&self, proposal: Proposal) {
        let mut proposals = self.proposals.write().expect("proposals lock poisoned");
        proposals.push(proposal);
    }

    /// Returns a clone of all proposals.
    pub fn proposals(&self) -> Vec<Proposal> {
        let proposals = self.proposals.read().expect("proposals lock poisoned");
        proposals.clone()
    }

    /// Updates the status of a proposal by ID. Returns `true` if the proposal was found.
    pub fn update_proposal_status(&self, proposal_id: &str, status: ProposalStatus) -> bool {
        let mut proposals = self.proposals.write().expect("proposals lock poisoned");
        if let Some(p) = proposals.iter_mut().find(|p| p.id == proposal_id) {
            p.status = status;
            true
        } else {
            false
        }
    }

    /// Casts a vote on a proposal by ID. Returns `true` if the proposal was found.
    pub fn vote_on_proposal(&self, proposal_id: &str, vote: Vote) -> bool {
        let mut proposals = self.proposals.write().expect("proposals lock poisoned");
        if let Some(p) = proposals.iter_mut().find(|p| p.id == proposal_id) {
            p.votes.push(vote);
            true
        } else {
            false
        }
    }

    /// Lists all keys currently in shared state.
    pub fn state_keys(&self) -> Vec<String> {
        let state = self.shared_state.read().expect("shared_state lock poisoned");
        state.keys().cloned().collect()
    }

    /// Builds a summary status snapshot of this collaboration.
    pub fn status(&self, agent_count: usize, active_agent: Option<String>) -> CollaborationStatus {
        let proposals = self.proposals.read().expect("proposals lock poisoned");
        let log = self.log.read().expect("log lock poisoned");
        let state = self.shared_state.read().expect("shared_state lock poisoned");

        CollaborationStatus {
            id: self.id.clone(),
            agent_count,
            active_agent,
            pending_proposals: proposals
                .iter()
                .filter(|p| p.status == ProposalStatus::Pending)
                .count(),
            event_count: log.len(),
            state_keys: state.keys().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> CollaborationContext {
        CollaborationContext::new("test-collab".to_string())
    }

    fn make_proposal(id: &str) -> Proposal {
        Proposal {
            id: id.to_string(),
            from_agent: "agent-1".to_string(),
            action: "refactor".to_string(),
            description: "Refactor module X".to_string(),
            status: ProposalStatus::Pending,
            votes: vec![],
        }
    }

    #[test]
    fn new_creates_empty_context() {
        let ctx = make_context();
        assert_eq!(ctx.id, "test-collab");
        assert!(ctx.events().is_empty());
        assert!(ctx.proposals().is_empty());
        assert!(ctx.state_keys().is_empty());
    }

    #[test]
    fn get_set_state() {
        let ctx = make_context();
        assert!(ctx.get_state("key1").is_none());

        ctx.set_state("key1".to_string(), serde_json::json!("value1"));
        assert_eq!(ctx.get_state("key1"), Some(serde_json::json!("value1")));

        // Overwrite
        ctx.set_state("key1".to_string(), serde_json::json!(42));
        assert_eq!(ctx.get_state("key1"), Some(serde_json::json!(42)));
    }

    #[test]
    fn remove_state() {
        let ctx = make_context();
        ctx.set_state("k".to_string(), serde_json::json!(true));

        let removed = ctx.remove_state("k");
        assert_eq!(removed, Some(serde_json::json!(true)));
        assert!(ctx.get_state("k").is_none());

        // Removing non-existent key returns None
        assert!(ctx.remove_state("missing").is_none());
    }

    #[test]
    fn log_event_and_events() {
        let ctx = make_context();
        ctx.log_event(CollaborationEvent::AgentJoined {
            agent_id: "a1".to_string(),
            capabilities: vec![AgentCapability::CodeGeneration],
        });
        ctx.log_event(CollaborationEvent::AgentLeft {
            agent_id: "a1".to_string(),
        });

        let events = ctx.events();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn log_event_message_sent() {
        let ctx = make_context();
        ctx.log_event(CollaborationEvent::MessageSent {
            from: "a1".to_string(),
            to: "a2".to_string(),
            content: "hello".to_string(),
        });
        assert_eq!(ctx.events().len(), 1);
    }

    #[test]
    fn log_event_task_delegated() {
        let ctx = make_context();
        ctx.log_event(CollaborationEvent::TaskDelegated {
            from: "a1".to_string(),
            to: "a2".to_string(),
            task: "implement feature".to_string(),
        });
        assert_eq!(ctx.events().len(), 1);
    }

    #[test]
    fn log_event_state_updated() {
        let ctx = make_context();
        ctx.log_event(CollaborationEvent::StateUpdated {
            agent_id: "a1".to_string(),
            key: "progress".to_string(),
        });
        assert_eq!(ctx.events().len(), 1);
    }

    #[test]
    fn add_and_list_proposals() {
        let ctx = make_context();
        ctx.add_proposal(make_proposal("p1"));
        ctx.add_proposal(make_proposal("p2"));

        let proposals = ctx.proposals();
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].id, "p1");
        assert_eq!(proposals[1].id, "p2");
    }

    #[test]
    fn update_proposal_status() {
        let ctx = make_context();
        ctx.add_proposal(make_proposal("p1"));

        assert!(ctx.update_proposal_status("p1", ProposalStatus::Accepted));
        let proposals = ctx.proposals();
        assert_eq!(proposals[0].status, ProposalStatus::Accepted);

        // Non-existent proposal returns false
        assert!(!ctx.update_proposal_status("missing", ProposalStatus::Rejected));
    }

    #[test]
    fn vote_on_proposal() {
        let ctx = make_context();
        ctx.add_proposal(make_proposal("p1"));

        let vote = Vote {
            agent_id: "agent-2".to_string(),
            approve: true,
            reason: Some("Looks good".to_string()),
        };
        assert!(ctx.vote_on_proposal("p1", vote));

        let proposals = ctx.proposals();
        assert_eq!(proposals[0].votes.len(), 1);
        assert!(proposals[0].votes[0].approve);

        // Vote on non-existent proposal returns false
        let vote2 = Vote {
            agent_id: "agent-3".to_string(),
            approve: false,
            reason: None,
        };
        assert!(!ctx.vote_on_proposal("missing", vote2));
    }

    #[test]
    fn state_keys() {
        let ctx = make_context();
        ctx.set_state("alpha".to_string(), serde_json::json!(1));
        ctx.set_state("beta".to_string(), serde_json::json!(2));

        let mut keys = ctx.state_keys();
        keys.sort();
        assert_eq!(keys, vec!["alpha", "beta"]);
    }

    #[test]
    fn collaboration_status_snapshot() {
        let ctx = make_context();
        ctx.set_state("x".to_string(), serde_json::json!(1));
        ctx.log_event(CollaborationEvent::AgentJoined {
            agent_id: "a1".to_string(),
            capabilities: vec![],
        });
        ctx.add_proposal(make_proposal("p1"));
        ctx.add_proposal(make_proposal("p2"));
        ctx.update_proposal_status("p2", ProposalStatus::Accepted);

        let status = ctx.status(3, Some("a1".to_string()));
        assert_eq!(status.id, "test-collab");
        assert_eq!(status.agent_count, 3);
        assert_eq!(status.active_agent, Some("a1".to_string()));
        assert_eq!(status.pending_proposals, 1); // only p1 is pending
        assert_eq!(status.event_count, 1);
        assert_eq!(status.state_keys.len(), 1);
    }

    #[test]
    fn proposal_serialization_roundtrip() {
        let proposal = make_proposal("p1");
        let json = serde_json::to_string(&proposal).unwrap();
        let decoded: Proposal = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "p1");
        assert_eq!(decoded.status, ProposalStatus::Pending);
    }

    #[test]
    fn collaboration_event_serialization() {
        let event = CollaborationEvent::AgentJoined {
            agent_id: "a1".to_string(),
            capabilities: vec![AgentCapability::CodeGeneration, AgentCapability::Testing],
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: CollaborationEvent = serde_json::from_str(&json).unwrap();
        // Verify it roundtrips (no panic)
        let _ = format!("{:?}", decoded);
    }

    #[test]
    fn collaboration_status_serialization() {
        let status = CollaborationStatus {
            id: "cs1".to_string(),
            agent_count: 2,
            active_agent: None,
            pending_proposals: 0,
            event_count: 5,
            state_keys: vec!["a".to_string()],
        };
        let json = serde_json::to_string(&status).unwrap();
        let decoded: CollaborationStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "cs1");
        assert_eq!(decoded.event_count, 5);
    }

    #[test]
    fn multiple_votes_on_same_proposal() {
        let ctx = make_context();
        ctx.add_proposal(make_proposal("p1"));

        for i in 0..5 {
            let vote = Vote {
                agent_id: format!("agent-{}", i),
                approve: i % 2 == 0,
                reason: None,
            };
            assert!(ctx.vote_on_proposal("p1", vote));
        }

        let proposals = ctx.proposals();
        assert_eq!(proposals[0].votes.len(), 5);
    }
}
