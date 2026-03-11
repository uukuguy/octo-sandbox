use serde::{Deserialize, Serialize};

use super::hlc::HlcTimestamp;

/// The type of mutation recorded in a sync change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncOperation {
    Insert,
    Update,
    Delete,
}

impl SyncOperation {
    /// Serialize to a string tag for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncOperation::Insert => "insert",
            SyncOperation::Update => "update",
            SyncOperation::Delete => "delete",
        }
    }

    /// Parse from a stored string tag.
    pub fn from_str_tag(s: &str) -> Option<Self> {
        match s {
            "insert" => Some(SyncOperation::Insert),
            "update" => Some(SyncOperation::Update),
            "delete" => Some(SyncOperation::Delete),
            _ => None,
        }
    }
}

/// A single change entry tracked in the sync changelog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncChange {
    /// Database row ID (populated when read from DB).
    pub id: Option<i64>,
    /// The table that was modified.
    pub table_name: String,
    /// Primary key of the affected row.
    pub row_id: String,
    /// Type of mutation.
    pub operation: SyncOperation,
    /// Payload — new values for Insert/Update, old values for Delete.
    pub data: serde_json::Value,
    /// HLC timestamp at the time of the change.
    pub hlc_timestamp: HlcTimestamp,
    /// Device that originated the change.
    pub device_id: String,
    /// Monotonic sync version counter.
    pub sync_version: u64,
}

/// Request from client to pull changes from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullRequest {
    /// Pull changes with sync_version > since_version.
    pub since_version: u64,
    /// Maximum number of changes to return.
    pub limit: usize,
}

/// Server response to a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullResponse {
    /// The changes since the requested version.
    pub changes: Vec<SyncChange>,
    /// Current server sync version high-water mark.
    pub server_version: u64,
    /// Whether more changes are available beyond the limit.
    pub has_more: bool,
}

/// Request from client to push local changes to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushRequest {
    /// Changes to push.
    pub changes: Vec<SyncChange>,
    /// Originating device ID.
    pub device_id: String,
    /// Client's current sync version.
    pub client_version: u64,
}

/// Server response to a push request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushResponse {
    /// Number of changes successfully applied.
    pub applied: usize,
    /// Conflicts encountered during push.
    pub conflicts: Vec<SyncConflict>,
    /// Server sync version after applying changes.
    pub server_version: u64,
}

/// Describes a conflict between client and server changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    /// Table where the conflict occurred.
    pub table_name: String,
    /// Row ID that conflicted.
    pub row_id: String,
    /// The value the client tried to write.
    pub client_value: serde_json::Value,
    /// The existing server value.
    pub server_value: serde_json::Value,
    /// How the conflict was resolved.
    pub resolution: ConflictResolution,
}

/// Strategy used to resolve a sync conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictResolution {
    ClientWins,
    ServerWins,
}

/// Current sync status for a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Device identifier.
    pub device_id: String,
    /// ISO-8601 timestamp of the last successful sync.
    pub last_sync_at: Option<String>,
    /// Latest sync version this device has seen.
    pub sync_version: u64,
    /// Number of local changes not yet pushed.
    pub pending_changes: u64,
}
