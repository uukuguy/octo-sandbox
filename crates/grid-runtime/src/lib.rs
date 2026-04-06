//! grid-runtime — EAASP L1 Agent Runtime (Tier 1 Harness)
//!
//! This crate implements the EAASP 13-method Runtime Contract as a gRPC
//! service. Grid is a Tier 1 Harness runtime with native hooks, MCP,
//! and skills support — zero adapter overhead.
//!
//! ## Architecture
//!
//! - `contract` — RuntimeContract trait + types (Rust-native form)
//! - `harness` — GridHarness: impl RuntimeContract via grid-engine (future)
//! - `service` — gRPC service mapping (future)
//!
//! ## Proto
//!
//! The gRPC service definition lives at `proto/eaasp/runtime/v1/runtime.proto`
//! and is compiled by `build.rs` via tonic-build.

pub mod contract;
pub mod harness;

/// Generated gRPC types from runtime.proto.
pub mod proto {
    tonic::include_proto!("eaasp.runtime.v1");
}
