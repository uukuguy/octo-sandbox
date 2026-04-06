//! eaasp-certifier — EAASP Runtime Contract verification library.
//!
//! Verifies that a gRPC endpoint correctly implements all 16 methods
//! of the EAASP RuntimeService contract.

pub mod mock_l3;
pub mod report;
pub mod runtime_pool;
pub mod selector;
pub mod verifier;

/// Generated gRPC types from common.proto.
pub mod common_proto {
    tonic::include_proto!("eaasp.common.v1");
}

/// Generated gRPC types from runtime.proto.
pub mod runtime_proto {
    tonic::include_proto!("eaasp.runtime.v1");
}
