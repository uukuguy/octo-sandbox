//! Sync client — pushes local changes and pulls remote changes from a server.
//!
//! [`SyncClient`] orchestrates a full bidirectional sync cycle:
//! 1. Push unsynced local changes to the server.
//! 2. Pull remote changes that happened since the last known version.
//! 3. Apply pulled changes locally via [`LwwResolver`].

use std::sync::Arc;

use anyhow::Result;
use tokio_rusqlite::Connection;

use super::changelog::ChangeTracker;
use super::hlc::HybridClock;
use super::lww::LwwResolver;
use super::protocol::*;

/// Report returned after a full sync cycle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    /// Number of changes the server accepted from us.
    pub pushed: usize,
    /// Number of remote changes we pulled down.
    pub pulled: usize,
    /// Conflicts encountered during pull-apply (already resolved via LWW).
    pub conflicts: Vec<SyncConflict>,
}

/// Client-side sync coordinator.
///
/// Holds an HTTP client, a [`ChangeTracker`] for local change bookkeeping,
/// and a [`HybridClock`] for causal ordering.
pub struct SyncClient {
    server_url: String,
    device_id: String,
    http: reqwest::Client,
    tracker: Arc<ChangeTracker>,
    clock: Arc<HybridClock>,
    conn: Connection,
}

impl SyncClient {
    /// Create a new sync client.
    ///
    /// * `server_url` — base URL of the remote octo-server (e.g. `http://localhost:3001`).
    /// * `device_id`  — unique identifier for this device / node.
    /// * `conn`       — local SQLite connection for applying remote changes.
    /// * `tracker`    — shared change tracker.
    /// * `clock`      — shared HLC instance.
    pub fn new(
        server_url: String,
        device_id: String,
        conn: Connection,
        tracker: Arc<ChangeTracker>,
        clock: Arc<HybridClock>,
    ) -> Self {
        Self {
            server_url,
            device_id,
            http: reqwest::Client::new(),
            tracker,
            clock,
            conn,
        }
    }

    /// Run a full sync cycle: push then pull.
    pub async fn sync(&self) -> Result<SyncReport> {
        // Push first so the server has our latest state before we pull.
        let push_result = self.push().await?;

        // Pull remote changes.
        let pull_result = self.pull().await?;

        // Apply pulled changes locally.
        let conflicts = if pull_result.changes.is_empty() {
            vec![]
        } else {
            LwwResolver::apply_remote_changes(&self.conn, &pull_result.changes, &self.clock)
                .await?
        };

        // Update local version bookmark.
        self.set_local_version(pull_result.server_version).await?;

        Ok(SyncReport {
            pushed: push_result.applied,
            pulled: pull_result.changes.len(),
            conflicts,
        })
    }

    // ── Push ────────────────────────────────────────────────────────────

    async fn push(&self) -> Result<SyncPushResponse> {
        let changes = self.tracker.get_unsynced_changes(1000).await?;

        if changes.is_empty() {
            return Ok(SyncPushResponse {
                applied: 0,
                conflicts: vec![],
                server_version: 0,
            });
        }

        let client_version = self.get_local_version().await?;

        let req = SyncPushRequest {
            changes: changes.clone(),
            device_id: self.device_id.clone(),
            client_version,
        };

        let resp = self
            .http
            .post(format!("{}/api/sync/push", self.server_url))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("sync push failed: {} — {}", status, body);
        }

        let push_resp: SyncPushResponse = resp.json().await?;

        // Mark successfully pushed changes as synced.
        let ids: Vec<i64> = changes.iter().filter_map(|c| c.id).collect();
        if !ids.is_empty() {
            self.tracker.mark_synced(&ids).await?;
        }

        Ok(push_resp)
    }

    // ── Pull ────────────────────────────────────────────────────────────

    async fn pull(&self) -> Result<SyncPullResponse> {
        let version = self.get_local_version().await?;

        let req = SyncPullRequest {
            since_version: version,
            limit: 1000,
        };

        let resp = self
            .http
            .post(format!("{}/api/sync/pull", self.server_url))
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("sync pull failed: {} — {}", status, body);
        }

        let pull_resp: SyncPullResponse = resp.json().await?;
        Ok(pull_resp)
    }

    // ── Local version bookkeeping ───────────────────────────────────────

    async fn get_local_version(&self) -> Result<u64> {
        let device_id = self.device_id.clone();

        let version = self
            .conn
            .call(move |conn| {
                // Ensure the table exists (idempotent).
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS sync_local_state (
                        key TEXT PRIMARY KEY,
                        value TEXT NOT NULL
                    )",
                )?;

                let mut stmt = conn.prepare_cached(
                    "SELECT value FROM sync_local_state WHERE key = ?1",
                )?;

                let result = stmt
                    .query_row(
                        rusqlite::params![format!("version:{}", device_id)],
                        |row| row.get::<_, String>(0),
                    )
                    .optional();

                match result {
                    Ok(Some(v)) => Ok(v.parse::<u64>().unwrap_or(0)),
                    _ => Ok(0u64),
                }
            })
            .await?;

        Ok(version)
    }

    async fn set_local_version(&self, version: u64) -> Result<()> {
        let device_id = self.device_id.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO sync_local_state (key, value)
                     VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![format!("version:{}", device_id), version.to_string()],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}

// Re-export optional for rusqlite
use rusqlite::OptionalExtension;
