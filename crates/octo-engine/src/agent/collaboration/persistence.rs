//! Persistence layer for collaboration context.
//!
//! Provides a [`CollaborationStore`] trait for saving/loading collaboration
//! snapshots and an [`InMemoryCollaborationStore`] implementation suitable
//! for testing and single-process deployments.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use octo_types::SessionId;

use super::context::{CollaborationEvent, Proposal};

/// A point-in-time snapshot of a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSnapshot {
    pub collaboration_id: String,
    pub shared_state: HashMap<String, serde_json::Value>,
    pub events: Vec<CollaborationEvent>,
    pub proposals: Vec<Proposal>,
    /// ISO 8601 timestamp of when the snapshot was saved.
    pub saved_at: String,
}

/// Storage trait for collaboration context persistence.
#[async_trait]
pub trait CollaborationStore: Send + Sync {
    /// Save collaboration state for a session.
    async fn save_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
        shared_state: &HashMap<String, serde_json::Value>,
        events: &[CollaborationEvent],
        proposals: &[Proposal],
    ) -> anyhow::Result<()>;

    /// Load collaboration state for a session.
    async fn load_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> anyhow::Result<Option<CollaborationSnapshot>>;

    /// List all collaboration IDs for a session.
    async fn list_collaborations(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<Vec<String>>;

    /// Delete collaboration data for a session.
    async fn delete_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> anyhow::Result<()>;
}

/// Composite key for the in-memory store: `(session_id, collaboration_id)`.
type StoreKey = (String, String);

/// In-memory implementation of [`CollaborationStore`].
///
/// Data lives only for the lifetime of the process.  Useful for tests and
/// single-session deployments where durability is not required.
pub struct InMemoryCollaborationStore {
    data: RwLock<HashMap<StoreKey, CollaborationSnapshot>>,
}

impl InMemoryCollaborationStore {
    /// Creates a new empty store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryCollaborationStore {
    fn default() -> Self {
        Self::new()
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[async_trait]
impl CollaborationStore for InMemoryCollaborationStore {
    async fn save_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
        shared_state: &HashMap<String, serde_json::Value>,
        events: &[CollaborationEvent],
        proposals: &[Proposal],
    ) -> anyhow::Result<()> {
        let snapshot = CollaborationSnapshot {
            collaboration_id: collaboration_id.to_string(),
            shared_state: shared_state.clone(),
            events: events.to_vec(),
            proposals: proposals.to_vec(),
            saved_at: now_iso8601(),
        };
        let key = (session_id.as_str().to_string(), collaboration_id.to_string());
        let mut data = self.data.write().await;
        data.insert(key, snapshot);
        Ok(())
    }

    async fn load_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> anyhow::Result<Option<CollaborationSnapshot>> {
        let key = (session_id.as_str().to_string(), collaboration_id.to_string());
        let data = self.data.read().await;
        Ok(data.get(&key).cloned())
    }

    async fn list_collaborations(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<Vec<String>> {
        let sid = session_id.as_str().to_string();
        let data = self.data.read().await;
        let ids: Vec<String> = data
            .keys()
            .filter(|(s, _)| s == &sid)
            .map(|(_, c)| c.clone())
            .collect();
        Ok(ids)
    }

    async fn delete_collaboration(
        &self,
        session_id: &SessionId,
        collaboration_id: &str,
    ) -> anyhow::Result<()> {
        let key = (session_id.as_str().to_string(), collaboration_id.to_string());
        let mut data = self.data.write().await;
        data.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session_id() -> SessionId {
        SessionId::new()
    }

    fn make_shared_state() -> HashMap<String, serde_json::Value> {
        let mut state = HashMap::new();
        state.insert("progress".to_string(), serde_json::json!(0.5));
        state.insert("phase".to_string(), serde_json::json!("planning"));
        state
    }

    fn make_events() -> Vec<CollaborationEvent> {
        vec![
            CollaborationEvent::AgentJoined {
                agent_id: "coder".to_string(),
                capabilities: vec![],
            },
            CollaborationEvent::MessageSent {
                from: "coder".to_string(),
                to: "reviewer".to_string(),
                content: "Ready for review".to_string(),
            },
        ]
    }

    fn make_proposals() -> Vec<Proposal> {
        use super::super::context::ProposalStatus;
        vec![Proposal {
            id: "p-1".to_string(),
            from_agent: "coder".to_string(),
            action: "refactor".to_string(),
            description: "Refactor auth module".to_string(),
            status: ProposalStatus::Pending,
            votes: vec![],
        }]
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let store = InMemoryCollaborationStore::new();
        let sid = make_session_id();
        let collab_id = "collab-1";
        let state = make_shared_state();
        let events = make_events();
        let proposals = make_proposals();

        store
            .save_collaboration(&sid, collab_id, &state, &events, &proposals)
            .await
            .unwrap();

        let loaded = store
            .load_collaboration(&sid, collab_id)
            .await
            .unwrap();

        assert!(loaded.is_some());
        let snapshot = loaded.unwrap();
        assert_eq!(snapshot.collaboration_id, "collab-1");
        assert_eq!(snapshot.shared_state.len(), 2);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.proposals.len(), 1);
        assert!(!snapshot.saved_at.is_empty());
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let store = InMemoryCollaborationStore::new();
        let sid = make_session_id();

        let result = store
            .load_collaboration(&sid, "does-not-exist")
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_collaborations() {
        let store = InMemoryCollaborationStore::new();
        let sid = make_session_id();
        let state = HashMap::new();

        store
            .save_collaboration(&sid, "collab-a", &state, &[], &[])
            .await
            .unwrap();
        store
            .save_collaboration(&sid, "collab-b", &state, &[], &[])
            .await
            .unwrap();

        // Different session should not appear
        let other_sid = make_session_id();
        store
            .save_collaboration(&other_sid, "collab-c", &state, &[], &[])
            .await
            .unwrap();

        let mut ids = store.list_collaborations(&sid).await.unwrap();
        ids.sort();
        assert_eq!(ids, vec!["collab-a", "collab-b"]);

        let other_ids = store.list_collaborations(&other_sid).await.unwrap();
        assert_eq!(other_ids, vec!["collab-c"]);
    }

    #[tokio::test]
    async fn delete_collaboration() {
        let store = InMemoryCollaborationStore::new();
        let sid = make_session_id();
        let state = HashMap::new();

        store
            .save_collaboration(&sid, "collab-x", &state, &[], &[])
            .await
            .unwrap();

        // Verify it exists
        assert!(store.load_collaboration(&sid, "collab-x").await.unwrap().is_some());

        // Delete it
        store.delete_collaboration(&sid, "collab-x").await.unwrap();

        // Verify it's gone
        assert!(store.load_collaboration(&sid, "collab-x").await.unwrap().is_none());

        // Deleting non-existent should not error
        store.delete_collaboration(&sid, "collab-x").await.unwrap();
    }

    #[tokio::test]
    async fn save_overwrites_existing() {
        let store = InMemoryCollaborationStore::new();
        let sid = make_session_id();

        let mut state1 = HashMap::new();
        state1.insert("version".to_string(), serde_json::json!(1));
        store
            .save_collaboration(&sid, "collab-1", &state1, &[], &[])
            .await
            .unwrap();

        let mut state2 = HashMap::new();
        state2.insert("version".to_string(), serde_json::json!(2));
        store
            .save_collaboration(&sid, "collab-1", &state2, &[], &[])
            .await
            .unwrap();

        let snapshot = store
            .load_collaboration(&sid, "collab-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.shared_state.get("version"), Some(&serde_json::json!(2)));
    }

    #[tokio::test]
    async fn snapshot_serialization_roundtrip() {
        let snapshot = CollaborationSnapshot {
            collaboration_id: "c-1".to_string(),
            shared_state: make_shared_state(),
            events: make_events(),
            proposals: make_proposals(),
            saved_at: now_iso8601(),
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: CollaborationSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.collaboration_id, "c-1");
        assert_eq!(decoded.shared_state.len(), 2);
        assert_eq!(decoded.events.len(), 2);
        assert_eq!(decoded.proposals.len(), 1);
    }
}
