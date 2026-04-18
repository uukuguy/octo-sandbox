pub mod adapter;
pub mod service;
pub mod ultra_worker;

pub mod proto {
    tonic::include_proto!("eaasp.runtime.v2");
}
