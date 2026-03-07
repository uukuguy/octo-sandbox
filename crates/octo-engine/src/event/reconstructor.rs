//! StateReconstructor — replays aggregate events to reconstruct state at a point in time.

use std::sync::Arc;

use crate::event::store::{EventStore, StoredEvent};

/// Domain aggregate state derived by replaying ordered events.
pub trait AggregateState: Send + Sync + Default {
    /// Apply a single stored event, mutating internal state.
    fn apply_event(&mut self, event: &StoredEvent);
}

/// Target endpoint for state reconstruction.
#[derive(Debug, Clone)]
pub enum ReconstructionPoint {
    /// Replay all events up to the latest sequence.
    Current,
    /// Replay events whose sequence number is <= the given value.
    AtSequence(i64),
    /// Replay events whose timestamp (ms since epoch) is <= the given value.
    AtTimestamp(i64),
}

/// Reconstructs aggregate state by replaying aggregate-scoped events from the
/// [`EventStore`].
///
/// Events are fetched via [`EventStore::read_by_aggregate`] and applied in
/// sequence order. An optional [`ReconstructionPoint`] lets callers recover
/// state at any historical moment.
pub struct StateReconstructor {
    store: Arc<EventStore>,
    /// Maximum number of events to fetch in a single reconstruction.
    max_events: usize,
}

impl StateReconstructor {
    /// Create a new reconstructor backed by the given store.
    pub fn new(store: Arc<EventStore>) -> Self {
        Self { store, max_events: 10_000 }
    }

    /// Override the maximum number of events fetched per reconstruction (default 10 000).
    pub fn with_max_events(mut self, max: usize) -> Self {
        self.max_events = max;
        self
    }

    /// Reconstruct aggregate state at the given [`ReconstructionPoint`].
    pub async fn reconstruct<S: AggregateState>(
        &self,
        aggregate_id: &str,
        point: ReconstructionPoint,
    ) -> anyhow::Result<S> {
        let all_events =
            self.store.read_by_aggregate(aggregate_id, 0, self.max_events).await?;
        // Warn callers when the store returned exactly max_events records: the
        // aggregate may have more events in the store, so the reconstructed
        // state could be partial.
        if all_events.len() == self.max_events {
            tracing::warn!(
                aggregate_id = %aggregate_id,
                max_events = self.max_events,
                "StateReconstructor: event limit reached, reconstructed state may be partial"
            );
        }
        let events = Self::apply_filter(all_events, &point);
        let mut state = S::default();
        for event in &events {
            state.apply_event(event);
        }
        tracing::debug!(
            aggregate_id,
            events_replayed = events.len(),
            "StateReconstructor complete"
        );
        Ok(state)
    }

    /// Convenience: reconstruct state up to (and including) `seq`.
    pub async fn at_sequence<S: AggregateState>(
        &self,
        aggregate_id: &str,
        seq: i64,
    ) -> anyhow::Result<S> {
        self.reconstruct(aggregate_id, ReconstructionPoint::AtSequence(seq)).await
    }

    /// Convenience: reconstruct state up to (and including) `ts_ms` (ms since epoch).
    pub async fn at_timestamp<S: AggregateState>(
        &self,
        aggregate_id: &str,
        ts_ms: i64,
    ) -> anyhow::Result<S> {
        self.reconstruct(aggregate_id, ReconstructionPoint::AtTimestamp(ts_ms)).await
    }

    fn apply_filter(events: Vec<StoredEvent>, point: &ReconstructionPoint) -> Vec<StoredEvent> {
        match point {
            ReconstructionPoint::Current => events,
            ReconstructionPoint::AtSequence(seq) => {
                events.into_iter().filter(|e| e.sequence <= *seq).collect()
            }
            ReconstructionPoint::AtTimestamp(ts) => {
                events.into_iter().filter(|e| e.timestamp <= *ts).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_rusqlite::Connection;

    #[derive(Default)]
    struct CallCounter {
        started: u32,
        completed: u32,
    }

    impl AggregateState for CallCounter {
        fn apply_event(&mut self, event: &StoredEvent) {
            match event.event_type.as_str() {
                "ToolCallStarted" => self.started += 1,
                "ToolCallCompleted" => self.completed += 1,
                _ => {}
            }
        }
    }

    async fn make_store() -> Arc<EventStore> {
        let conn = Connection::open_in_memory().await.unwrap();
        Arc::new(EventStore::new(conn).await.unwrap())
    }

    #[tokio::test]
    async fn test_reconstruct_current() {
        let store = make_store().await;
        store
            .append("ToolCallStarted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();
        store
            .append("ToolCallStarted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();
        store
            .append("ToolCallCompleted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();
        // Event for a different aggregate — must NOT appear in agg-1 result.
        store
            .append("ToolCallStarted", serde_json::json!({}), Some("agg-2"), None, None)
            .await
            .unwrap();

        let rec = StateReconstructor::new(store);
        let state: CallCounter =
            rec.reconstruct("agg-1", ReconstructionPoint::Current).await.unwrap();
        assert_eq!(state.started, 2);
        assert_eq!(state.completed, 1);
    }

    #[tokio::test]
    async fn test_reconstruct_at_sequence() {
        let store = make_store().await;
        store
            .append("ToolCallStarted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();
        store
            .append("ToolCallStarted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();
        store
            .append("ToolCallCompleted", serde_json::json!({}), Some("agg-1"), None, None)
            .await
            .unwrap();

        let rec = StateReconstructor::new(store);
        // Reconstruct at sequence 2 — only the first two events should be applied.
        let state: CallCounter = rec.at_sequence("agg-1", 2).await.unwrap();
        assert_eq!(state.started, 2);
        assert_eq!(state.completed, 0);
    }
}
