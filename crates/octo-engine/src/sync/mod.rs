//! Offline-first sync module (D6 protocol).
//!
//! Implements LWW (Last-Writer-Wins) conflict resolution with Hybrid Logical
//! Clocks (HLC) for causal ordering across devices.
//!
//! # Sub-modules
//!
//! | Module       | Responsibility                                    |
//! |-------------|---------------------------------------------------|
//! | `hlc`       | Hybrid Logical Clock implementation                |
//! | `protocol`  | Wire-format types (requests, responses, changes)   |
//! | `changelog` | Local change tracking and bookkeeping              |
//! | `lww`       | Last-Writer-Wins conflict resolver                 |
//! | `server`    | Server-side pull/push handler                      |
//! | `client`    | Client-side sync orchestrator (HTTP)               |

pub mod changelog;
pub mod client;
pub mod hlc;
pub mod lww;
pub mod protocol;
pub mod server;

// Re-exports for convenience
pub use changelog::ChangeTracker;
pub use client::{SyncClient, SyncReport};
pub use hlc::{HlcTimestamp, HybridClock};
pub use lww::LwwResolver;
pub use protocol::{
    ConflictResolution, SyncChange, SyncConflict, SyncOperation, SyncPullRequest,
    SyncPullResponse, SyncPushRequest, SyncPushResponse, SyncStatus,
};
pub use server::SyncServer;
