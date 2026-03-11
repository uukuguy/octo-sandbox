//! Local change tracking for the sync system.
//!
//! [`ChangeTracker`] records mutations into `sync_changelog` so they can be
//! pushed to a remote server during the next sync cycle.

use std::sync::Arc;

use anyhow::Result;
use tracing::debug;

use super::hlc::{HlcTimestamp, HybridClock};
use super::protocol::{SyncChange, SyncOperation};

/// Tracks local data changes in the sync_changelog table so they can
/// be replicated to remote peers.
pub struct ChangeTracker {
    conn: tokio_rusqlite::Connection,
    clock: Arc<HybridClock>,
    device_id: String,
}

impl ChangeTracker {
    /// Create a new change tracker backed by the given database connection.
    pub fn new(
        conn: tokio_rusqlite::Connection,
        clock: Arc<HybridClock>,
        device_id: String,
    ) -> Self {
        Self {
            conn,
            clock,
            device_id,
        }
    }

    /// Record a mutation in the sync changelog.
    ///
    /// Returns the database row ID of the inserted changelog entry.
    pub async fn record_change(
        &self,
        table: &str,
        row_id: &str,
        op: SyncOperation,
        _old: Option<serde_json::Value>,
        new: Option<serde_json::Value>,
    ) -> Result<i64> {
        let ts = self.clock.now();
        let device_id = self.device_id.clone();
        let table = table.to_string();
        let row_id = row_id.to_string();
        let op_str = op.as_str().to_string();
        let data_json = new
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        let node_id = ts.node_id.clone();
        let physical_ms = ts.physical_ms;
        let logical = ts.logical;

        debug!(
            table = %table,
            row_id = %row_id,
            operation = %op_str,
            "Recording sync change"
        );

        let id = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO sync_changelog \
                     (table_name, row_id, operation, data, \
                      hlc_physical_ms, hlc_logical, node_id, \
                      device_id, synced) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
                    rusqlite::params![
                        table, row_id, op_str, data_json,
                        physical_ms, logical, node_id,
                        device_id,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(id)
    }

    /// Retrieve unsynced changes, ordered by ID, up to the given limit.
    pub async fn get_unsynced_changes(&self, limit: usize) -> Result<Vec<SyncChange>> {
        let changes = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, table_name, row_id, operation, data, \
                     hlc_physical_ms, hlc_logical, node_id, \
                     device_id, sync_version \
                     FROM sync_changelog WHERE synced = 0 \
                     ORDER BY id ASC LIMIT ?1",
                )?;
                let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok(SyncChange {
                        id: row.get(0)?,
                        table_name: row.get(1)?,
                        row_id: row.get(2)?,
                        operation: SyncOperation::from_str_tag(
                            &row.get::<_, String>(3)?,
                        )
                        .unwrap_or(SyncOperation::Update),
                        data: serde_json::from_str(&row.get::<_, String>(4)?)
                            .unwrap_or_default(),
                        hlc_timestamp: HlcTimestamp {
                            physical_ms: row.get(5)?,
                            logical: row.get(6)?,
                            node_id: row.get(7)?,
                        },
                        device_id: row.get(8)?,
                        sync_version: row.get(9)?,
                    })
                })?;
                let changes: Vec<SyncChange> = rows.filter_map(|r| r.ok()).collect();
                Ok(changes)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(changes)
    }

    /// Mark the given changelog entries as synced.
    pub async fn mark_synced(&self, change_ids: &[i64]) -> Result<()> {
        if change_ids.is_empty() {
            return Ok(());
        }
        let ids = change_ids.to_vec();
        self.conn
            .call(move |conn| {
                let placeholders: Vec<String> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", i + 1))
                    .collect();
                let sql = format!(
                    "UPDATE sync_changelog SET synced = 1 WHERE id IN ({})",
                    placeholders.join(", ")
                );
                let params: Vec<Box<dyn rusqlite::types::ToSql>> = ids
                    .iter()
                    .map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>)
                    .collect();
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                conn.execute(&sql, param_refs.as_slice())?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(())
    }

    /// Get all changes with sync_version greater than `version`.
    pub async fn get_changes_since(&self, version: u64) -> Result<Vec<SyncChange>> {
        let changes = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, table_name, row_id, operation, data, \
                     hlc_physical_ms, hlc_logical, node_id, \
                     device_id, sync_version \
                     FROM sync_changelog WHERE sync_version > ?1 \
                     ORDER BY id ASC",
                )?;
                let rows = stmt.query_map(rusqlite::params![version as i64], |row| {
                    Ok(SyncChange {
                        id: row.get(0)?,
                        table_name: row.get(1)?,
                        row_id: row.get(2)?,
                        operation: SyncOperation::from_str_tag(
                            &row.get::<_, String>(3)?,
                        )
                        .unwrap_or(SyncOperation::Update),
                        data: serde_json::from_str(&row.get::<_, String>(4)?)
                            .unwrap_or_default(),
                        hlc_timestamp: HlcTimestamp {
                            physical_ms: row.get(5)?,
                            logical: row.get(6)?,
                            node_id: row.get(7)?,
                        },
                        device_id: row.get(8)?,
                        sync_version: row.get(9)?,
                    })
                })?;
                let changes: Vec<SyncChange> = rows.filter_map(|r| r.ok()).collect();
                Ok(changes)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(changes)
    }

    /// Delete synced changelog entries older than the given number of days.
    /// Returns the number of rows deleted.
    pub async fn cleanup_old_changes(&self, older_than_days: u32) -> Result<usize> {
        let cutoff_ms =
            chrono::Utc::now().timestamp_millis() - (older_than_days as i64 * 86_400_000);

        let count = self
            .conn
            .call(move |conn| {
                let deleted = conn.execute(
                    "DELETE FROM sync_changelog \
                     WHERE synced = 1 AND hlc_physical_ms < ?1",
                    rusqlite::params![cutoff_ms],
                )?;
                Ok(deleted)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(count)
    }

    /// Return the count of unsynced changes.
    pub async fn pending_count(&self) -> Result<u64> {
        let count = self
            .conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM sync_changelog WHERE synced = 0",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count as u64)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(count)
    }
}
