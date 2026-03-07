use serde::{Deserialize, Serialize};
use tokio_rusqlite::Connection;
use tracing::{debug, instrument};

/// A persisted event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub aggregate_id: Option<String>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: i64,
    pub sequence: i64,
}

/// Append-only event store backed by SQLite.
///
/// Provides persistent, ordered event storage with queries by sequence,
/// session, and event type. Designed for event sourcing replay and
/// projection rebuilding.
pub struct EventStore {
    conn: Connection,
}

impl EventStore {
    /// Create a new EventStore, initializing the schema if needed.
    pub async fn new(conn: Connection) -> anyhow::Result<Self> {
        conn.call(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    aggregate_id TEXT,
                    event_type TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    session_id TEXT,
                    agent_id TEXT,
                    timestamp INTEGER NOT NULL,
                    sequence INTEGER NOT NULL UNIQUE
                );
                CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
                CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
                CREATE INDEX IF NOT EXISTS idx_events_sequence ON events(sequence);
                CREATE INDEX IF NOT EXISTS idx_events_aggregate ON events(aggregate_id);",
            )?;
            Ok(())
        })
        .await?;

        debug!("EventStore initialized");
        Ok(Self { conn })
    }

    /// Append an event, returning the assigned sequence number.
    #[instrument(skip(self, payload), fields(event_type = %event_type))]
    pub async fn append(
        &self,
        event_type: &str,
        payload: serde_json::Value,
        aggregate_id: Option<&str>,
        session_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> anyhow::Result<i64> {
        let event_type = event_type.to_string();
        let payload_str = serde_json::to_string(&payload)?;
        let aggregate_id = aggregate_id.map(|s| s.to_string());
        let session_id = session_id.map(|s| s.to_string());
        let agent_id = agent_id.map(|s| s.to_string());
        let timestamp = chrono::Utc::now().timestamp_millis();

        let sequence = self
            .conn
            .call(move |c| {
                // Use a transaction to ensure atomicity of sequence assignment
                let tx = c.transaction()?;

                // Get next sequence number
                let next_seq: i64 = tx
                    .query_row(
                        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events",
                        [],
                        |row| row.get(0),
                    )?;

                tx.execute(
                    "INSERT INTO events (aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![aggregate_id, event_type, payload_str, session_id, agent_id, timestamp, next_seq],
                )?;

                tx.commit()?;
                Ok(next_seq)
            })
            .await?;

        debug!(sequence, "Event appended");
        Ok(sequence)
    }

    /// Read events after a given sequence number (for replay).
    pub async fn read_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredEvent>> {
        let limit = limit as i64;
        self.conn
            .call(move |c| {
                let mut stmt = c.prepare(
                    "SELECT id, aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence
                     FROM events
                     WHERE sequence > ?1
                     ORDER BY sequence ASC
                     LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![after_sequence, limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(Into::into)
    }

    /// Read events for a specific session.
    pub async fn read_by_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredEvent>> {
        let session_id = session_id.to_string();
        let limit = limit as i64;
        self.conn
            .call(move |c| {
                let mut stmt = c.prepare(
                    "SELECT id, aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence
                     FROM events
                     WHERE session_id = ?1
                     ORDER BY sequence ASC
                     LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![session_id, limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(Into::into)
    }

    /// Read events of a specific type.
    pub async fn read_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredEvent>> {
        let event_type = event_type.to_string();
        let limit = limit as i64;
        self.conn
            .call(move |c| {
                let mut stmt = c.prepare(
                    "SELECT id, aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence
                     FROM events
                     WHERE event_type = ?1
                     ORDER BY sequence ASC
                     LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![event_type, limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(Into::into)
    }

    /// Get the latest sequence number (0 if no events).
    pub async fn latest_sequence(&self) -> anyhow::Result<i64> {
        self.conn
            .call(|c| {
                let seq: i64 = c.query_row(
                    "SELECT COALESCE(MAX(sequence), 0) FROM events",
                    [],
                    |row| row.get(0),
                )?;
                Ok(seq)
            })
            .await
            .map_err(Into::into)
    }

    /// Count total events in the store.
    pub async fn count(&self) -> anyhow::Result<i64> {
        self.conn
            .call(|c| {
                let count: i64 =
                    c.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
                Ok(count)
            })
            .await
            .map_err(Into::into)
    }

    /// Read events for a specific aggregate, ordered by sequence.
    ///
    /// Used by `StateReconstructor` to replay aggregate-scoped event streams.
    pub async fn read_by_aggregate(
        &self,
        aggregate_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredEvent>> {
        let aggregate_id = aggregate_id.to_string();
        let limit = limit as i64;
        self.conn
            .call(move |c| {
                let mut stmt = c.prepare(
                    "SELECT id, aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence
                     FROM events
                     WHERE aggregate_id = ?1 AND sequence > ?2
                     ORDER BY sequence ASC
                     LIMIT ?3",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![aggregate_id, after_sequence, limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(Into::into)
    }
}

/// Map a SQLite row to a StoredEvent.
/// Column order: 0=id, 1=aggregate_id, 2=event_type, 3=payload, 4=session_id, 5=agent_id, 6=timestamp, 7=sequence
fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEvent> {
    let payload_str: String = row.get(3)?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
    Ok(StoredEvent {
        id: row.get(0)?,
        aggregate_id: row.get(1)?,
        event_type: row.get(2)?,
        payload,
        session_id: row.get(4)?,
        agent_id: row.get(5)?,
        timestamp: row.get(6)?,
        sequence: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_conn() -> Connection {
        Connection::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let conn = test_conn().await;
        let store = EventStore::new(conn).await.unwrap();

        let seq1 = store
            .append(
                "ToolCallStarted",
                serde_json::json!({"tool": "bash"}),
                None,
                Some("session-1"),
                Some("agent-1"),
            )
            .await
            .unwrap();
        assert_eq!(seq1, 1);

        let seq2 = store
            .append(
                "ToolCallCompleted",
                serde_json::json!({"tool": "bash", "duration_ms": 42}),
                None,
                Some("session-1"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(seq2, 2);

        // read_after
        let events = store.read_after(0, 100).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "ToolCallStarted");
        assert_eq!(events[1].sequence, 2);

        // read_after with offset
        let events = store.read_after(1, 100).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "ToolCallCompleted");

        // read_by_session
        let events = store.read_by_session("session-1", 100).await.unwrap();
        assert_eq!(events.len(), 2);

        // read_by_type
        let events = store.read_by_type("ToolCallStarted", 100).await.unwrap();
        assert_eq!(events.len(), 1);

        // latest_sequence
        assert_eq!(store.latest_sequence().await.unwrap(), 2);

        // count
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_empty_store() {
        let conn = test_conn().await;
        let store = EventStore::new(conn).await.unwrap();

        assert_eq!(store.latest_sequence().await.unwrap(), 0);
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(store.read_after(0, 100).await.unwrap().is_empty());
    }
}
