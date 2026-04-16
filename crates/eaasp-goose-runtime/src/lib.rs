pub mod goose_adapter;
pub mod service;

pub mod proto {
    tonic::include_proto!("eaasp.runtime.v2");
}
