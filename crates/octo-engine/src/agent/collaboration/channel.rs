use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Message types for agent-to-agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollaborationMessage {
    /// Request another agent to execute a task.
    DelegateTask {
        task_id: String,
        description: String,
    },
    /// Return task result.
    TaskResult {
        task_id: String,
        result: String,
        success: bool,
    },
    /// Share context information.
    ShareContext {
        key: String,
        value: serde_json::Value,
    },
    /// Request information from another agent.
    RequestInfo {
        request_id: String,
        query: String,
    },
    /// Response to an info request.
    InfoResponse {
        request_id: String,
        response: String,
    },
    /// Notify about a proposal.
    ProposalNotification {
        proposal_id: String,
        action: String,
        from_agent: String,
    },
    /// Byzantine consensus protocol message — leader proposes.
    ConsensusProposal {
        proposal_id: String,
        action: String,
        description: String,
        proposer: String,
        total_agents: usize,
    },
    /// Prepare phase vote in Byzantine consensus.
    PrepareVote {
        proposal_id: String,
        agent_id: String,
        approve: bool,
    },
    /// Commit phase vote in Byzantine consensus.
    CommitVote {
        proposal_id: String,
        agent_id: String,
        approve: bool,
    },
    /// Consensus result notification.
    ConsensusResult {
        proposal_id: String,
        finalized: bool,
        phase: String,
    },
    /// Signed consensus vote (Prepare phase) with ED25519 signature.
    SignedPrepareVote {
        proposal_id: String,
        agent_id: String,
        approve: bool,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    },
    /// Signed consensus vote (Commit phase) with ED25519 signature.
    SignedCommitVote {
        proposal_id: String,
        agent_id: String,
        approve: bool,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    },
    /// View change request from a replica.
    ViewChangeRequest {
        from_agent: String,
        current_view: u64,
        proposed_view: u64,
        reason: String,
    },
    /// New view announcement (broadcast by the new leader).
    NewView {
        view_number: u64,
        new_leader: String,
    },
}

/// A bidirectional communication channel between two agents.
pub struct CollaborationChannel {
    /// Agent ID of the sender side.
    pub from_agent: String,
    /// Agent ID of the receiver side.
    pub to_agent: String,
    /// Sender half.
    tx: mpsc::Sender<CollaborationMessage>,
    /// Receiver half (wrapped in Option for take pattern).
    rx: Option<mpsc::Receiver<CollaborationMessage>>,
}

impl CollaborationChannel {
    /// Create a new unidirectional channel from `from_agent` to `to_agent`.
    pub fn new(from_agent: String, to_agent: String, buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        Self {
            from_agent,
            to_agent,
            tx,
            rx: Some(rx),
        }
    }

    /// Send a message through this channel.
    pub async fn send(
        &self,
        msg: CollaborationMessage,
    ) -> Result<(), mpsc::error::SendError<CollaborationMessage>> {
        self.tx.send(msg).await
    }

    /// Receive a message, waiting asynchronously until one is available.
    /// Returns `None` if all senders have been dropped.
    pub async fn recv(&mut self) -> Option<CollaborationMessage> {
        self.rx.as_mut()?.recv().await
    }

    /// Try to receive a message without blocking.
    /// Returns `None` if no message is available or the receiver has been taken.
    pub fn try_recv(&mut self) -> Option<CollaborationMessage> {
        self.rx.as_mut()?.try_recv().ok()
    }

    /// Clone the sender half so other tasks can also send into this channel.
    pub fn sender(&self) -> mpsc::Sender<CollaborationMessage> {
        self.tx.clone()
    }
}

/// Creates a pair of channels for bidirectional communication between two agents.
///
/// Returns `(channel_a_to_b, channel_b_to_a)` where:
/// - `channel_a_to_b`: agent_a sends via `send()`, agent_b receives via `recv()`
/// - `channel_b_to_a`: agent_b sends via `send()`, agent_a receives via `recv()`
///
/// Internally two independent mpsc channels are created and cross-wired so that
/// each agent's tx connects to the other agent's rx.
pub fn create_channel_pair(
    agent_a: &str,
    agent_b: &str,
    buffer_size: usize,
) -> (CollaborationChannel, CollaborationChannel) {
    let (tx_a, rx_a) = mpsc::channel(buffer_size);
    let (tx_b, rx_b) = mpsc::channel(buffer_size);

    let channel_a_to_b = CollaborationChannel {
        from_agent: agent_a.to_string(),
        to_agent: agent_b.to_string(),
        tx: tx_a,
        rx: Some(rx_b),
    };

    let channel_b_to_a = CollaborationChannel {
        from_agent: agent_b.to_string(),
        to_agent: agent_a.to_string(),
        tx: tx_b,
        rx: Some(rx_a),
    };

    (channel_a_to_b, channel_b_to_a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_creation() {
        let ch = CollaborationChannel::new("agent-a".into(), "agent-b".into(), 16);
        assert_eq!(ch.from_agent, "agent-a");
        assert_eq!(ch.to_agent, "agent-b");
        assert!(ch.rx.is_some());
    }

    #[tokio::test]
    async fn test_send_and_receive() {
        let mut ch = CollaborationChannel::new("a".into(), "b".into(), 8);

        ch.send(CollaborationMessage::ShareContext {
            key: "hello".into(),
            value: serde_json::json!("world"),
        })
        .await
        .unwrap();

        let msg = ch.recv().await.unwrap();
        match msg {
            CollaborationMessage::ShareContext { key, value } => {
                assert_eq!(key, "hello");
                assert_eq!(value, serde_json::json!("world"));
            }
            other => panic!("unexpected message: {:?}", other),
        }
    }

    #[test]
    fn test_try_recv_when_empty() {
        let mut ch = CollaborationChannel::new("a".into(), "b".into(), 8);
        assert!(ch.try_recv().is_none());
    }

    #[tokio::test]
    async fn test_create_channel_pair_bidirectional() {
        let (mut ch_a, mut ch_b) = create_channel_pair("alice", "bob", 8);

        assert_eq!(ch_a.from_agent, "alice");
        assert_eq!(ch_a.to_agent, "bob");
        assert_eq!(ch_b.from_agent, "bob");
        assert_eq!(ch_b.to_agent, "alice");

        // alice sends to bob
        ch_a.send(CollaborationMessage::DelegateTask {
            task_id: "t1".into(),
            description: "do something".into(),
        })
        .await
        .unwrap();

        // bob receives from alice
        let msg = ch_b.recv().await.unwrap();
        match msg {
            CollaborationMessage::DelegateTask {
                task_id,
                description,
            } => {
                assert_eq!(task_id, "t1");
                assert_eq!(description, "do something");
            }
            other => panic!("unexpected: {:?}", other),
        }

        // bob sends to alice
        ch_b.send(CollaborationMessage::TaskResult {
            task_id: "t1".into(),
            result: "done".into(),
            success: true,
        })
        .await
        .unwrap();

        // alice receives from bob
        let msg = ch_a.recv().await.unwrap();
        match msg {
            CollaborationMessage::TaskResult {
                task_id,
                result,
                success,
            } => {
                assert_eq!(task_id, "t1");
                assert_eq!(result, "done");
                assert!(success);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_sender_clone_works() {
        let mut ch = CollaborationChannel::new("a".into(), "b".into(), 8);
        let cloned_tx = ch.sender();

        // Send via cloned sender
        cloned_tx
            .send(CollaborationMessage::RequestInfo {
                request_id: "r1".into(),
                query: "status?".into(),
            })
            .await
            .unwrap();

        // Receive via original channel
        let msg = ch.recv().await.unwrap();
        match msg {
            CollaborationMessage::RequestInfo {
                request_id,
                query,
            } => {
                assert_eq!(request_id, "r1");
                assert_eq!(query, "status?");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }
}
