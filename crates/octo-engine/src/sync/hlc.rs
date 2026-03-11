use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

/// A Hybrid Logical Clock timestamp combining physical wall-clock time,
/// a logical counter for ordering within the same millisecond, and
/// a node identifier for tie-breaking across devices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HlcTimestamp {
    pub physical_ms: i64,
    pub logical: u32,
    pub node_id: String,
}

impl Ord for HlcTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.physical_ms
            .cmp(&other.physical_ms)
            .then(self.logical.cmp(&other.logical))
            .then(self.node_id.cmp(&other.node_id))
    }
}

impl PartialOrd for HlcTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for HlcTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.physical_ms, self.logical, self.node_id)
    }
}

/// Error returned when parsing an HLC timestamp string fails.
#[derive(Debug, Clone)]
pub struct HlcParseError(pub String);

impl fmt::Display for HlcParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid HLC timestamp: {}", self.0)
    }
}

impl std::error::Error for HlcParseError {}

impl FromStr for HlcTimestamp {
    type Err = HlcParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Err(HlcParseError(format!(
                "expected format physical_ms:logical:node_id, got '{s}'"
            )));
        }
        let physical_ms = parts[0]
            .parse::<i64>()
            .map_err(|e| HlcParseError(format!("invalid physical_ms: {e}")))?;
        let logical = parts[1]
            .parse::<u32>()
            .map_err(|e| HlcParseError(format!("invalid logical: {e}")))?;
        let node_id = parts[2].to_string();
        if node_id.is_empty() {
            return Err(HlcParseError("node_id cannot be empty".into()));
        }
        Ok(HlcTimestamp {
            physical_ms,
            logical,
            node_id,
        })
    }
}

/// A Hybrid Logical Clock that produces monotonically increasing timestamps.
///
/// Safe for concurrent use via atomic operations.
pub struct HybridClock {
    physical: AtomicI64,
    logical: AtomicU32,
    node_id: String,
}

impl HybridClock {
    /// Create a new HybridClock for the given node.
    pub fn new(node_id: String) -> Self {
        Self {
            physical: AtomicI64::new(0),
            logical: AtomicU32::new(0),
            node_id,
        }
    }

    /// Generate a new monotonically increasing timestamp.
    pub fn now(&self) -> HlcTimestamp {
        let wall = chrono::Utc::now().timestamp_millis();
        let prev_physical = self.physical.load(Ordering::SeqCst);

        if wall > prev_physical {
            self.physical.store(wall, Ordering::SeqCst);
            self.logical.store(0, Ordering::SeqCst);
            HlcTimestamp {
                physical_ms: wall,
                logical: 0,
                node_id: self.node_id.clone(),
            }
        } else {
            let l = self.logical.fetch_add(1, Ordering::SeqCst) + 1;
            HlcTimestamp {
                physical_ms: prev_physical,
                logical: l,
                node_id: self.node_id.clone(),
            }
        }
    }

    /// Update the clock after receiving a remote timestamp.
    /// Ensures the local clock stays ahead of both wall clock and
    /// the remote timestamp.
    pub fn update(&self, remote: &HlcTimestamp) {
        let wall = chrono::Utc::now().timestamp_millis();
        let prev = self.physical.load(Ordering::SeqCst);
        let max_pt = wall.max(prev).max(remote.physical_ms);

        if max_pt == prev && max_pt == remote.physical_ms {
            let local_l = self.logical.load(Ordering::SeqCst);
            let l = local_l.max(remote.logical) + 1;
            self.logical.store(l, Ordering::SeqCst);
        } else if max_pt == prev {
            self.logical.fetch_add(1, Ordering::SeqCst);
        } else if max_pt == remote.physical_ms {
            self.physical.store(max_pt, Ordering::SeqCst);
            self.logical.store(remote.logical + 1, Ordering::SeqCst);
        } else {
            // wall clock is ahead of both
            self.physical.store(max_pt, Ordering::SeqCst);
            self.logical.store(0, Ordering::SeqCst);
        }
    }

    /// Return the node ID of this clock.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}
