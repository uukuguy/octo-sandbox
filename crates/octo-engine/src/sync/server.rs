//! Sync server — handles pull/push requests from remote clients.
//!
//! The [`SyncServer`] wraps a [`ChangeTracker`] and [`LwwResolver`] to provide
//! a high-level API that maps 1-to-1 onto the REST endpoints exposed by
//! `octo-server`.

use std::sync::Arc;

use anyhow::Result;
use tokio_rusqlite::Connection;

use super::changelog::ChangeTracker;
use super::hlc::HybridClock;
use super::lww::LwwResolver;
use super::protocol::*;

/// Server-side sync coordinator.
///
/// Accepts pull/push requests, resolves conflicts via LWW, and maintains
/// per-device sync metadata.
pub struct SyncServer {
    conn: Connection,
    clock: Arc<HybridClock>,
    tracker: Arc<ChangeTracker>,
}

impl SyncServer {
    pub fn new(conn: Connection, clock: Arc<HybridClock>, tracker: Arc<ChangeTracker>) -> Self {
        Self {
            conn,
            clock,
            tracker,
        }
    }

    /// Handle a pull request: return changes since the requested version.
    ///
    /// The response is paginated via `req.limit`. When `has_more` is `true`
    /// the client should issue another pull with the highest version it
    /// received.
    pub async fn handle_pull(&self, req: SyncPullRequest) -> Result<SyncPullResponse> {
        let all_changes = self.tracker.get_changes_since(req.since_version).await?;

        let limited: Vec<SyncChange> = all_changes.into_iter().take(req.limit).collect();
        let has_more = limited.len() == req.limit;
        let server_version = self.get_server_version().await?;

        Ok(SyncPullResponse {
            changes: limited,
            server_version,
            has_more,
        })
    }

    /// Handle a push request: apply remote changes with LWW conflict resolution.
    ///
    /// Returns the number of successfully applied changes plus any conflicts
    /// that were resolved (with their resolution strategy).
    pub async fn handle_push(&self, req: SyncPushRequest) -> Result<SyncPushResponse> {
        let conflicts =
            LwwResolver::apply_remote_changes(&self.conn, &req.changes, &self.clock).await?;
        let applied = req.changes.len() - conflicts.len();

        self.update_device_metadata(&req.device_id).await?;
        let server_version = self.get_server_version().await?;

        Ok(SyncPushResponse {
            applied,
            conflicts,
            server_version,
        })
    }

    /// Return the current sync status for a specific device.
    pub async fn get_status(&self, device_id: &str) -> Result<SyncStatus> {
        let did = device_id.to_string();
        let tracker = self.tracker.clone();

        let (last_sync_at, sync_version) = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT last_sync_at, sync_version FROM sync_metadata WHERE device_id = ?1",
                )?;

                let result = stmt
                    .query_row(rusqlite::params![did], |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, u64>(1)?,
                        ))
                    })
                    .optional();

                match result {
                    Ok(Some(row)) => Ok(row),
                    Ok(None) | Err(_) => Ok((None, 0u64)),
                }
            })
            .await?;

        let pending_changes = tracker.pending_count().await?;

        Ok(SyncStatus {
            device_id: device_id.to_string(),
            last_sync_at,
            sync_version,
            pending_changes,
        })
    }

    // ── Private helpers ─────────────────────────────────────────────────

    /// Read the maximum `sync_version` across all changelog entries.
    async fn get_server_version(&self) -> Result<u64> {
        let version = self
            .conn
            .call(|conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT COALESCE(MAX(sync_version), 0) FROM sync_changelog",
                )?;
                let v: u64 = stmt.query_row([], |row| row.get(0))?;
                Ok(v)
            })
            .await?;
        Ok(version)
    }

    /// Upsert per-device metadata after a successful push.
    async fn update_device_metadata(&self, device_id: &str) -> Result<()> {
        let did = device_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO sync_metadata (device_id, last_sync_at, sync_version)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(sync_version), 0) FROM sync_changelog))
                     ON CONFLICT(device_id) DO UPDATE SET
                       last_sync_at = excluded.last_sync_at,
                       sync_version = excluded.sync_version",
                    rusqlite::params![did, now],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}

/// Standalone helpers that mirror `SyncServer` methods but accept a raw
/// `Connection`.  Used by the REST handlers in `octo-server` where
/// constructing a full `SyncServer` would require an `Arc<ChangeTracker>`.
pub mod standalone {
    use anyhow::Result;
    use rusqlite::OptionalExtension;
    use tokio_rusqlite::Connection;

    use super::super::protocol::*;

    /// Pull changes since `since_version` (max `limit` rows).
    pub async fn pull(conn: &Connection, req: &SyncPullRequest) -> Result<SyncPullResponse> {
        let since = req.since_version;
        let limit = req.limit;

        let (changes, server_version) = conn
            .call(move |conn| {
                // Fetch changes
                let mut stmt = conn.prepare_cached(
                    "SELECT id, table_name, row_id, operation, data,
                            hlc_physical_ms, hlc_logical, node_id,
                            device_id, sync_version
                     FROM sync_changelog
                     WHERE sync_version > ?1
                     ORDER BY sync_version ASC
                     LIMIT ?2",
                )?;

                let rows = stmt.query_map(rusqlite::params![since, limit], |row| {
                    Ok(SyncChange {
                        id: row.get(0)?,
                        table_name: row.get(1)?,
                        row_id: row.get(2)?,
                        operation: match row.get::<_, String>(3)?.as_str() {
                            "insert" => SyncOperation::Insert,
                            "delete" => SyncOperation::Delete,
                            _ => SyncOperation::Update,
                        },
                        data: serde_json::from_str(&row.get::<_, String>(4)?)
                            .unwrap_or_default(),
                        hlc_timestamp: super::super::hlc::HlcTimestamp {
                            physical_ms: row.get(5)?,
                            logical: row.get(6)?,
                            node_id: row.get(7)?,
                        },
                        device_id: row.get(8)?,
                        sync_version: row.get(9)?,
                    })
                })?;

                let changes: Vec<SyncChange> = rows.filter_map(|r| r.ok()).collect();

                // Server version
                let sv: u64 = conn
                    .prepare_cached(
                        "SELECT COALESCE(MAX(sync_version), 0) FROM sync_changelog",
                    )?
                    .query_row([], |r| r.get(0))?;

                Ok((changes, sv))
            })
            .await?;

        let has_more = changes.len() == limit;

        Ok(SyncPullResponse {
            changes,
            server_version,
            has_more,
        })
    }

    /// Push remote changes and return the result.
    ///
    /// This is a simplified version that inserts changes directly into the
    /// changelog. Full LWW conflict resolution is performed by `LwwResolver`
    /// in the engine, but for the REST endpoint we do a lightweight insert
    /// so that the server at least records the incoming changes.
    pub async fn push(conn: &Connection, req: &SyncPushRequest) -> Result<SyncPushResponse> {
        let changes = req.changes.clone();
        let device_id = req.device_id.clone();

        let (applied, server_version) = conn
            .call(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let mut count = 0usize;

                for change in &changes {
                    let op_str = match change.operation {
                        SyncOperation::Insert => "insert",
                        SyncOperation::Update => "update",
                        SyncOperation::Delete => "delete",
                    };
                    let data_str = serde_json::to_string(&change.data).unwrap_or_default();

                    let result = tx.execute(
                        "INSERT INTO sync_changelog
                            (table_name, row_id, operation, data,
                             hlc_physical_ms, hlc_logical, node_id,
                             device_id, synced)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
                        rusqlite::params![
                            change.table_name,
                            change.row_id,
                            op_str,
                            data_str,
                            change.hlc_timestamp.physical_ms,
                            change.hlc_timestamp.logical,
                            change.hlc_timestamp.node_id,
                            change.device_id,
                        ],
                    );
                    if result.is_ok() {
                        count += 1;
                    }
                }

                // Upsert device metadata
                let now = chrono::Utc::now().to_rfc3339();
                tx.execute(
                    "INSERT INTO sync_metadata (device_id, last_sync_at, sync_version)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(sync_version), 0) FROM sync_changelog))
                     ON CONFLICT(device_id) DO UPDATE SET
                       last_sync_at = excluded.last_sync_at,
                       sync_version = excluded.sync_version",
                    rusqlite::params![device_id, now],
                )?;

                tx.commit()?;

                let sv: u64 = conn
                    .prepare_cached(
                        "SELECT COALESCE(MAX(sync_version), 0) FROM sync_changelog",
                    )?
                    .query_row([], |r| r.get(0))?;

                Ok((count, sv))
            })
            .await?;

        Ok(SyncPushResponse {
            applied,
            conflicts: vec![],
            server_version,
        })
    }

    /// Get sync status for a device.
    pub async fn status(conn: &Connection, device_id: &str) -> Result<SyncStatus> {
        let did = device_id.to_string();

        let status = conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT last_sync_at, sync_version FROM sync_metadata WHERE device_id = ?1",
                )?;

                let result = stmt
                    .query_row(rusqlite::params![did.clone()], |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, u64>(1)?,
                        ))
                    })
                    .optional();

                let (last_sync_at, sync_version) = match result {
                    Ok(Some(row)) => row,
                    _ => (None, 0u64),
                };

                let pending: u64 = conn
                    .prepare_cached(
                        "SELECT COUNT(*) FROM sync_changelog WHERE device_id = ?1 AND synced = 0",
                    )?
                    .query_row(rusqlite::params![did], |r| r.get(0))
                    .unwrap_or(0);

                Ok(SyncStatus {
                    device_id: did,
                    last_sync_at,
                    sync_version,
                    pending_changes: pending,
                })
            })
            .await?;

        Ok(status)
    }

    /// Ensure the sync tables exist (idempotent).
    pub async fn ensure_tables(conn: &Connection) -> Result<()> {
        conn.call(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sync_changelog (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    table_name TEXT NOT NULL,
                    row_id TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    data TEXT NOT NULL DEFAULT '{}',
                    hlc_physical_ms INTEGER NOT NULL DEFAULT 0,
                    hlc_logical INTEGER NOT NULL DEFAULT 0,
                    node_id TEXT NOT NULL DEFAULT '',
                    device_id TEXT NOT NULL DEFAULT '',
                    sync_version INTEGER NOT NULL DEFAULT 0,
                    synced INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS sync_metadata (
                    device_id TEXT PRIMARY KEY,
                    last_sync_at TEXT,
                    sync_version INTEGER NOT NULL DEFAULT 0
                );

                -- Auto-assign sync_version on insert via trigger
                CREATE TRIGGER IF NOT EXISTS sync_changelog_version
                AFTER INSERT ON sync_changelog
                WHEN NEW.sync_version = 0
                BEGIN
                    UPDATE sync_changelog
                    SET sync_version = NEW.id
                    WHERE id = NEW.id;
                END;",
            )?;
            Ok(())
        })
        .await?;
        Ok(())
    }
}

// Re-export optional for rusqlite
use rusqlite::OptionalExtension;
