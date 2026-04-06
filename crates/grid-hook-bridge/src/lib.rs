//! grid-hook-bridge — EAASP HookBridge for L1 runtime hook evaluation.
//!
//! Provides two modes:
//! - `InProcessHookBridge` — in-process evaluation (testing, T1 simulation)
//! - `GrpcHookBridge` — gRPC client to external HookBridge sidecar (T2/T3)
//!
//! Also includes `HookBridgeGrpcServer` — gRPC server for sidecar deployment.

pub mod grpc_bridge;
pub mod in_process;
pub mod server;
pub mod traits;

/// Generated gRPC types from common.proto.
pub mod common_proto {
    tonic::include_proto!("eaasp.common.v1");
}

/// Generated gRPC types from hook.proto.
pub mod hook_proto {
    tonic::include_proto!("eaasp.hook.v1");
}
