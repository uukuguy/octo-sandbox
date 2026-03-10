//! High-level collaboration protocol operations.
//!
//! Wraps low-level channel messaging and context manipulation into
//! semantic actions such as proposing, voting, delegating, and sharing state.

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::channel::{CollaborationChannel, CollaborationMessage};
use super::context::{CollaborationContext, CollaborationEvent, Proposal, ProposalStatus, Vote};

/// Monotonic counter to ensure unique IDs even within the same nanosecond.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a unique ID with the given prefix based on the current timestamp
/// and a monotonic counter.
fn generate_id(prefix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{:x}-{}", prefix, ts, seq)
}

/// High-level collaboration protocol operations.
///
/// All methods are stateless — the struct itself holds no data.
/// State lives in [`CollaborationContext`] and messages flow through
/// [`CollaborationChannel`].
pub struct CollaborationProtocol;

impl CollaborationProtocol {
    /// Propose an action to the collaboration.
    ///
    /// Creates a new [`Proposal`] with [`ProposalStatus::Pending`] and adds it
    /// to the collaboration context. Returns the generated proposal ID.
    pub fn propose_action(
        context: &CollaborationContext,
        from_agent: &str,
        action: String,
        description: String,
    ) -> String {
        let proposal_id = generate_id("prop");
        let proposal = Proposal {
            id: proposal_id.clone(),
            from_agent: from_agent.to_string(),
            action,
            description,
            status: ProposalStatus::Pending,
            votes: vec![],
        };
        context.add_proposal(proposal);
        proposal_id
    }

    /// Cast a vote on an existing proposal.
    ///
    /// Returns `true` if the proposal was found, `false` otherwise.
    pub fn vote(
        context: &CollaborationContext,
        proposal_id: &str,
        agent_id: &str,
        approve: bool,
        reason: Option<String>,
    ) -> bool {
        let vote = Vote {
            agent_id: agent_id.to_string(),
            approve,
            reason,
        };
        context.vote_on_proposal(proposal_id, vote)
    }

    /// Accept a proposal (update its status to [`ProposalStatus::Accepted`]).
    ///
    /// Returns `true` if the proposal was found.
    pub fn accept_proposal(context: &CollaborationContext, proposal_id: &str) -> bool {
        context.update_proposal_status(proposal_id, ProposalStatus::Accepted)
    }

    /// Reject a proposal (update its status to [`ProposalStatus::Rejected`]).
    ///
    /// Returns `true` if the proposal was found.
    pub fn reject_proposal(context: &CollaborationContext, proposal_id: &str) -> bool {
        context.update_proposal_status(proposal_id, ProposalStatus::Rejected)
    }

    /// Delegate a task to another agent via the communication channel.
    ///
    /// Sends a [`CollaborationMessage::DelegateTask`] message and logs a
    /// [`CollaborationEvent::TaskDelegated`] event. Returns the generated task ID.
    pub async fn delegate_task(
        channel: &CollaborationChannel,
        context: &CollaborationContext,
        from_agent: &str,
        to_agent: &str,
        task_description: String,
    ) -> Result<String> {
        let task_id = generate_id("task");
        channel
            .send(CollaborationMessage::DelegateTask {
                task_id: task_id.clone(),
                description: task_description.clone(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send delegate task: {}", e))?;

        context.log_event(CollaborationEvent::TaskDelegated {
            from: from_agent.to_string(),
            to: to_agent.to_string(),
            task: task_description,
        });

        Ok(task_id)
    }

    /// Share state data with all agents via the collaboration context.
    ///
    /// Sets the key-value pair in shared state and logs a
    /// [`CollaborationEvent::StateUpdated`] event.
    pub fn share_state(
        context: &CollaborationContext,
        agent_id: &str,
        key: String,
        value: serde_json::Value,
    ) {
        context.set_state(key.clone(), value);
        context.log_event(CollaborationEvent::StateUpdated {
            agent_id: agent_id.to_string(),
            key,
        });
    }

    /// Request information from another agent.
    ///
    /// Sends a [`CollaborationMessage::RequestInfo`] through the channel.
    pub async fn request_info(
        channel: &CollaborationChannel,
        request_id: String,
        query: String,
    ) -> Result<()> {
        channel
            .send(CollaborationMessage::RequestInfo {
                request_id,
                query,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send info request: {}", e))
    }

    /// Respond to an information request.
    ///
    /// Sends a [`CollaborationMessage::InfoResponse`] through the channel.
    pub async fn respond_info(
        channel: &CollaborationChannel,
        request_id: String,
        response: String,
    ) -> Result<()> {
        channel
            .send(CollaborationMessage::InfoResponse {
                request_id,
                response,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send info response: {}", e))
    }

    /// Return all proposals that are still [`ProposalStatus::Pending`].
    pub fn pending_proposals(context: &CollaborationContext) -> Vec<Proposal> {
        context
            .proposals()
            .into_iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::collaboration::channel::create_channel_pair;

    fn make_context() -> CollaborationContext {
        CollaborationContext::new("test-proto".to_string())
    }

    #[test]
    fn propose_action_creates_pending_proposal() {
        let ctx = make_context();
        let id = CollaborationProtocol::propose_action(
            &ctx,
            "agent-1",
            "refactor".to_string(),
            "Refactor module X".to_string(),
        );

        assert!(id.starts_with("prop-"));
        let proposals = ctx.proposals();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].id, id);
        assert_eq!(proposals[0].from_agent, "agent-1");
        assert_eq!(proposals[0].action, "refactor");
        assert_eq!(proposals[0].description, "Refactor module X");
        assert_eq!(proposals[0].status, ProposalStatus::Pending);
        assert!(proposals[0].votes.is_empty());
    }

    #[test]
    fn vote_adds_vote_to_proposal() {
        let ctx = make_context();
        let id = CollaborationProtocol::propose_action(
            &ctx,
            "agent-1",
            "action".to_string(),
            "desc".to_string(),
        );

        let found =
            CollaborationProtocol::vote(&ctx, &id, "agent-2", true, Some("LGTM".to_string()));
        assert!(found);

        let proposals = ctx.proposals();
        assert_eq!(proposals[0].votes.len(), 1);
        assert_eq!(proposals[0].votes[0].agent_id, "agent-2");
        assert!(proposals[0].votes[0].approve);
        assert_eq!(
            proposals[0].votes[0].reason,
            Some("LGTM".to_string())
        );
    }

    #[test]
    fn vote_on_missing_proposal_returns_false() {
        let ctx = make_context();
        let found = CollaborationProtocol::vote(&ctx, "nonexistent", "agent-1", true, None);
        assert!(!found);
    }

    #[test]
    fn accept_proposal_changes_status() {
        let ctx = make_context();
        let id = CollaborationProtocol::propose_action(
            &ctx,
            "agent-1",
            "a".to_string(),
            "d".to_string(),
        );

        assert!(CollaborationProtocol::accept_proposal(&ctx, &id));
        let proposals = ctx.proposals();
        assert_eq!(proposals[0].status, ProposalStatus::Accepted);
    }

    #[test]
    fn reject_proposal_changes_status() {
        let ctx = make_context();
        let id = CollaborationProtocol::propose_action(
            &ctx,
            "agent-1",
            "a".to_string(),
            "d".to_string(),
        );

        assert!(CollaborationProtocol::reject_proposal(&ctx, &id));
        let proposals = ctx.proposals();
        assert_eq!(proposals[0].status, ProposalStatus::Rejected);
    }

    #[test]
    fn accept_missing_proposal_returns_false() {
        let ctx = make_context();
        assert!(!CollaborationProtocol::accept_proposal(&ctx, "missing"));
    }

    #[test]
    fn reject_missing_proposal_returns_false() {
        let ctx = make_context();
        assert!(!CollaborationProtocol::reject_proposal(&ctx, "missing"));
    }

    #[tokio::test]
    async fn delegate_task_sends_message_and_logs_event() {
        let (ch_a, mut ch_b) = create_channel_pair("alice", "bob", 8);
        let ctx = make_context();

        let task_id = CollaborationProtocol::delegate_task(
            &ch_a,
            &ctx,
            "alice",
            "bob",
            "implement feature Y".to_string(),
        )
        .await
        .unwrap();

        assert!(task_id.starts_with("task-"));

        // Verify the message was sent
        let msg = ch_b.recv().await.unwrap();
        match msg {
            CollaborationMessage::DelegateTask {
                task_id: tid,
                description,
            } => {
                assert_eq!(tid, task_id);
                assert_eq!(description, "implement feature Y");
            }
            other => panic!("unexpected message: {:?}", other),
        }

        // Verify the event was logged
        let events = ctx.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            CollaborationEvent::TaskDelegated { from, to, task } => {
                assert_eq!(from, "alice");
                assert_eq!(to, "bob");
                assert_eq!(task, "implement feature Y");
            }
            other => panic!("unexpected event: {:?}", other),
        }
    }

    #[test]
    fn share_state_updates_context_and_logs_event() {
        let ctx = make_context();
        CollaborationProtocol::share_state(
            &ctx,
            "agent-1",
            "progress".to_string(),
            serde_json::json!(75),
        );

        assert_eq!(ctx.get_state("progress"), Some(serde_json::json!(75)));

        let events = ctx.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            CollaborationEvent::StateUpdated { agent_id, key } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(key, "progress");
            }
            other => panic!("unexpected event: {:?}", other),
        }
    }

    #[test]
    fn pending_proposals_filters_correctly() {
        let ctx = make_context();
        let id1 = CollaborationProtocol::propose_action(
            &ctx,
            "a1",
            "action1".to_string(),
            "desc1".to_string(),
        );
        let _id2 = CollaborationProtocol::propose_action(
            &ctx,
            "a2",
            "action2".to_string(),
            "desc2".to_string(),
        );
        let id3 = CollaborationProtocol::propose_action(
            &ctx,
            "a3",
            "action3".to_string(),
            "desc3".to_string(),
        );

        // Accept first, reject third — only second should remain pending
        CollaborationProtocol::accept_proposal(&ctx, &id1);
        CollaborationProtocol::reject_proposal(&ctx, &id3);

        let pending = CollaborationProtocol::pending_proposals(&ctx);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].from_agent, "a2");
        assert_eq!(pending[0].status, ProposalStatus::Pending);
    }

    #[tokio::test]
    async fn request_info_sends_message() {
        let (ch_a, mut ch_b) = create_channel_pair("alice", "bob", 8);

        CollaborationProtocol::request_info(&ch_a, "req-1".to_string(), "status?".to_string())
            .await
            .unwrap();

        let msg = ch_b.recv().await.unwrap();
        match msg {
            CollaborationMessage::RequestInfo {
                request_id,
                query,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(query, "status?");
            }
            other => panic!("unexpected message: {:?}", other),
        }
    }

    #[tokio::test]
    async fn respond_info_sends_message() {
        let (ch_a, mut ch_b) = create_channel_pair("alice", "bob", 8);

        CollaborationProtocol::respond_info(
            &ch_a,
            "req-1".to_string(),
            "all good".to_string(),
        )
        .await
        .unwrap();

        let msg = ch_b.recv().await.unwrap();
        match msg {
            CollaborationMessage::InfoResponse {
                request_id,
                response,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(response, "all good");
            }
            other => panic!("unexpected message: {:?}", other),
        }
    }
}
