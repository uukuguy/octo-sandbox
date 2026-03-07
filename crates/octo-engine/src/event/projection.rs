use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::store::{EventStore, StoredEvent};

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

// ── ProjectionEngine ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct ProjectionCheckpoint {
    last_sequence: i64,
}

/// Engine that drives one or more [`Projection`]s over the full event stream.
///
/// Tracks a per-projection checkpoint so catch-up replays only process new
/// events. Supports full rebuild by resetting checkpoints to zero.
///
/// `catch_up_lock` serializes concurrent `catch_up()` calls so that two callers
/// cannot simultaneously read the same checkpoint and double-apply the same
/// batch of events to stateful projections such as [`EventCountProjection`].
pub struct ProjectionEngine {
    store: Arc<EventStore>,
    projections: RwLock<Vec<Arc<dyn Projection>>>,
    checkpoints: RwLock<HashMap<String, ProjectionCheckpoint>>,
    replay_batch: usize,
    /// Mutex that serializes concurrent catch_up() invocations.
    catch_up_lock: Arc<tokio::sync::Mutex<()>>,
}

impl ProjectionEngine {
    /// Create a new engine backed by the given [`EventStore`].
    pub fn new(store: Arc<EventStore>) -> Self {
        Self {
            store,
            projections: RwLock::new(Vec::new()),
            checkpoints: RwLock::new(HashMap::new()),
            replay_batch: 500,
            catch_up_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Override the batch size used during catch-up (default 500).
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.replay_batch = size;
        self
    }

    /// Register a projection. An initial checkpoint at sequence 0 is created
    /// if none exists yet.
    pub async fn register(&self, projection: Arc<dyn Projection>) {
        let name = projection.name().to_string();
        self.projections.write().await.push(projection);
        self.checkpoints.write().await.entry(name).or_default();
    }

    /// Process all events since each projection's last checkpoint.
    ///
    /// Acquires `catch_up_lock` before reading the checkpoint so that
    /// concurrent callers are serialized and cannot double-apply events.
    pub async fn catch_up(&self) -> anyhow::Result<()> {
        let _guard = self.catch_up_lock.lock().await;
        let projections = self.projections.read().await;
        if projections.is_empty() {
            return Ok(());
        }
        // Start from the minimum checkpoint across all registered projections.
        let start_seq = {
            let cps = self.checkpoints.read().await;
            projections
                .iter()
                .map(|p| cps.get(p.name()).map_or(0, |c| c.last_sequence))
                .min()
                .unwrap_or(0)
        };
        let mut after = start_seq;
        loop {
            let batch = self.store.read_after(after, self.replay_batch).await?;
            if batch.is_empty() {
                break;
            }
            let last_seq = batch.last().unwrap().sequence;
            for event in &batch {
                for proj in projections.iter() {
                    let proj_last = self
                        .checkpoints
                        .read()
                        .await
                        .get(proj.name())
                        .map_or(0, |c| c.last_sequence);
                    if event.sequence > proj_last {
                        proj.apply(event).await?;
                        self.checkpoints.write().await.insert(
                            proj.name().to_string(),
                            ProjectionCheckpoint { last_sequence: event.sequence },
                        );
                    }
                }
            }
            after = last_seq;
            if batch.len() < self.replay_batch {
                break;
            }
        }
        Ok(())
    }

    /// Reset all checkpoints to zero and replay the full event stream.
    pub async fn rebuild_all(&self) -> anyhow::Result<()> {
        {
            let mut cps = self.checkpoints.write().await;
            for cp in cps.values_mut() {
                cp.last_sequence = 0;
            }
        }
        self.catch_up().await
    }

    /// Return the last processed sequence number for a named projection.
    pub async fn checkpoint(&self, name: &str) -> i64 {
        self.checkpoints.read().await.get(name).map_or(0, |c| c.last_sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: &str, seq: i64) -> StoredEvent {
        StoredEvent {
            id: seq,
            aggregate_id: None,
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
