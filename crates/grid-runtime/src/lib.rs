//! grid-runtime — EAASP L1 Agent Runtime (Tier 1 Harness)
//!
//! This crate implements the EAASP 16-method Runtime Contract as a gRPC
//! service. Grid is a Tier 1 Harness runtime with native hooks, MCP,
//! and skills support — zero adapter overhead.
//!
//! ## Architecture
//!
//! - `contract` — RuntimeContract trait + types (Rust-native form)
//! - `harness` — GridHarness: impl RuntimeContract via grid-engine
//! - `service` — gRPC service mapping
//! - `telemetry` — EAASP telemetry event collection and conversion
//!
//! ## Proto
//!
//! The gRPC service definition lives at `proto/eaasp/runtime/v1/runtime.proto`
//! with shared types in `proto/eaasp/common/v1/common.proto`.
//! Both are compiled by `build.rs` via tonic-build.

pub mod config;
pub mod contract;
pub mod harness;
pub mod service;
pub mod telemetry;

/// Generated gRPC types from common.proto (shared types).
pub mod common_proto {
    tonic::include_proto!("eaasp.common.v1");
}

/// Generated gRPC types from runtime.proto.
pub mod proto {
    tonic::include_proto!("eaasp.runtime.v1");
}
