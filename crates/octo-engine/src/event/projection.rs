use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::store::StoredEvent;

/// Trait for event projections -- derive read models from event streams.
///
/// Projections consume events and maintain a materialized view that can be
/// queried efficiently. They support full rebuild from an event stream.
#[async_trait]
pub trait Projection: Send + Sync {
    /// Name of this projection.
    fn name(&self) -> &str;

    /// Process a single event, updating internal state.
    async fn apply(&self, event: &StoredEvent) -> anyhow::Result<()>;

    /// Rebuild projection from a stream of events (replay).
    async fn rebuild(&self, events: &[StoredEvent]) -> anyhow::Result<()> {
        for event in events {
            self.apply(event).await?;
        }
        Ok(())
    }
}

/// Built-in projection: counts events by type.
pub struct EventCountProjection {
    counts: RwLock<HashMap<String, u64>>,
}

impl EventCountProjection {
    pub fn new() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
        }
    }

    /// Get a snapshot of all event type counts.
    pub async fn get_counts(&self) -> HashMap<String, u64> {
        self.counts.read().await.clone()
    }

    /// Get the count for a specific event type.
    pub async fn get_count(&self, event_type: &str) -> u64 {
        self.counts
            .read()
            .await
            .get(event_type)
            .copied()
            .unwrap_or(0)
    }
}

impl Default for EventCountProjection {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projection for EventCountProjection {
    fn name(&self) -> &str {
        "event-count"
    }

    async fn apply(&self, event: &StoredEvent) -> anyhow::Result<()> {
        let mut counts = self.counts.write().await;
        *counts.entry(event.event_type.clone()).or_insert(0) += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: &str, seq: i64) -> StoredEvent {
        StoredEvent {
            id: seq,
            event_type: event_type.to_string(),
            payload: serde_json::Value::Null,
            session_id: None,
            agent_id: None,
            timestamp: 0,
            sequence: seq,
        }
    }

    #[tokio::test]
    async fn test_event_count_projection() {
        let proj = EventCountProjection::new();

        proj.apply(&make_event("ToolCallStarted", 1)).await.unwrap();
        proj.apply(&make_event("ToolCallStarted", 2)).await.unwrap();
        proj.apply(&make_event("ToolCallCompleted", 3))
            .await
            .unwrap();

        assert_eq!(proj.get_count("ToolCallStarted").await, 2);
        assert_eq!(proj.get_count("ToolCallCompleted").await, 1);
        assert_eq!(proj.get_count("NonExistent").await, 0);

        let counts = proj.get_counts().await;
        assert_eq!(counts.len(), 2);
    }

    #[tokio::test]
    async fn test_rebuild() {
        let proj = EventCountProjection::new();
        let events = vec![
            make_event("A", 1),
            make_event("B", 2),
            make_event("A", 3),
            make_event("A", 4),
        ];

        proj.rebuild(&events).await.unwrap();

        assert_eq!(proj.get_count("A").await, 3);
        assert_eq!(proj.get_count("B").await, 1);
    }
}
