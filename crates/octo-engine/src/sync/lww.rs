//! Last-Writer-Wins conflict resolver for the sync system.
//!
//! Resolution rules:
//! 1. DELETE always wins over non-DELETE (tombstone semantics).
//! 2. When both operations are the same type, the one with the higher HLC
//!    timestamp wins.

use std::sync::Arc;

use anyhow::Result;
use tracing::debug;

use super::hlc::{HlcTimestamp, HybridClock};
use super::protocol::{ConflictResolution, SyncChange, SyncConflict, SyncOperation};

/// Last-Writer-Wins conflict resolver.
pub struct LwwResolver;

impl LwwResolver {
    /// Resolve a conflict between a local and a remote change.
    ///
    /// `local` represents the client-side change, `remote` the server-side.
    pub fn resolve(local: &SyncChange, remote: &SyncChange) -> ConflictResolution {
        // DELETE wins over non-DELETE (tombstone semantics)
        if remote.operation == SyncOperation::Delete && local.operation != SyncOperation::Delete {
            return ConflictResolution::ServerWins;
        }
        if local.operation == SyncOperation::Delete && remote.operation != SyncOperation::Delete {
            return ConflictResolution::ClientWins;
        }

        // Both same type: compare HLC timestamps, newer wins
        if remote.hlc_timestamp > local.hlc_timestamp {
            ConflictResolution::ServerWins
        } else {
            ConflictResolution::ClientWins
        }
    }

    /// Apply a batch of remote changes to the local database, resolving
    /// any conflicts using Last-Writer-Wins semantics.
    ///
    /// Returns a list of all conflicts that were encountered (regardless
    /// of which side won).
    pub async fn apply_remote_changes(
        conn: &tokio_rusqlite::Connection,
        changes: &[SyncChange],
        clock: &Arc<HybridClock>,
    ) -> Result<Vec<SyncConflict>> {
        let changes_owned: Vec<SyncChange> = changes.to_vec();
        let clock = Arc::clone(clock);

        let conflicts = conn
            .call(move |conn| {
                let mut conflicts = Vec::new();

                for remote_change in &changes_owned {
                    // Update local HLC clock with the remote timestamp
                    clock.update(&remote_change.hlc_timestamp);

                    // Check for conflicting unsynced local changes
                    let local_conflict = find_local_conflict(
                        conn,
                        &remote_change.table_name,
                        &remote_change.row_id,
                    )?;

                    if let Some(local_change) = local_conflict {
                        let resolution = LwwResolver::resolve(&local_change, remote_change);

                        debug!(
                            table = %remote_change.table_name,
                            row_id = %remote_change.row_id,
                            resolution = ?resolution,
                            "Sync conflict resolved"
                        );

                        conflicts.push(SyncConflict {
                            table_name: remote_change.table_name.clone(),
                            row_id: remote_change.row_id.clone(),
                            client_value: local_change.data.clone(),
                            server_value: remote_change.data.clone(),
                            resolution: resolution.clone(),
                        });

                        if resolution == ConflictResolution::ClientWins {
                            // Local wins — skip applying remote change
                            continue;
                        }
                    }

                    // Apply the remote change by recording it as a synced entry
                    apply_change(conn, remote_change)?;
                }

                Ok(conflicts)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(conflicts)
    }
}

/// Look for an unsynced local change that conflicts with the given
/// (table_name, row_id) pair.
fn find_local_conflict(
    conn: &rusqlite::Connection,
    table_name: &str,
    row_id: &str,
) -> rusqlite::Result<Option<SyncChange>> {
    let mut stmt = conn.prepare(
        "SELECT id, table_name, row_id, operation, data, \
         hlc_physical_ms, hlc_logical, node_id, \
         device_id, sync_version \
         FROM sync_changelog \
         WHERE table_name = ?1 AND row_id = ?2 AND synced = 0 \
         ORDER BY id DESC LIMIT 1",
    )?;

    let mut rows = stmt.query(rusqlite::params![table_name, row_id])?;
    if let Some(row) = rows.next()? {
        let op_str: String = row.get(3)?;
        Ok(Some(SyncChange {
            id: row.get(0)?,
            table_name: row.get(1)?,
            row_id: row.get(2)?,
            operation: SyncOperation::from_str_tag(&op_str).unwrap_or(SyncOperation::Update),
            data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
            hlc_timestamp: HlcTimestamp {
                physical_ms: row.get(5)?,
                logical: row.get(6)?,
                node_id: row.get(7)?,
            },
            device_id: row.get(8)?,
            sync_version: row.get(9)?,
        }))
    } else {
        Ok(None)
    }
}

/// Record a remote change locally as an already-synced changelog entry.
fn apply_change(conn: &rusqlite::Connection, change: &SyncChange) -> rusqlite::Result<()> {
    let op_str = change.operation.as_str();
    let data_str = serde_json::to_string(&change.data).unwrap_or_else(|_| "{}".to_string());

    conn.execute(
        "INSERT INTO sync_changelog \
         (table_name, row_id, operation, data, \
          hlc_physical_ms, hlc_logical, node_id, \
          device_id, sync_version, synced) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
        rusqlite::params![
            change.table_name,
            change.row_id,
            op_str,
            data_str,
            change.hlc_timestamp.physical_ms,
            change.hlc_timestamp.logical,
            change.hlc_timestamp.node_id,
            change.device_id,
            change.sync_version as i64,
        ],
    )?;

    Ok(())
}
