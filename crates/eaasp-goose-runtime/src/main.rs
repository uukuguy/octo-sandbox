//! eaasp-goose-runtime gRPC server entry point.
//!
//! Listens on GOOSE_RUNTIME_GRPC_ADDR (default 0.0.0.0:50053).
//! Reads EAASP_DEPLOYMENT_MODE and passes it into GooseAdapter.

use std::sync::Arc;

use tonic::transport::Server;

use eaasp_goose_runtime::goose_adapter::GooseAdapter;
use eaasp_goose_runtime::proto::runtime_service_server::RuntimeServiceServer;
use eaasp_goose_runtime::service::GooseRuntimeService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let deployment_mode = std::env::var("EAASP_DEPLOYMENT_MODE")
        .unwrap_or_else(|_| "shared".to_string());

    let addr = std::env::var("GOOSE_RUNTIME_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50053".to_string())
        .parse()?;

    tracing::info!(
        addr = %addr,
        mode = %deployment_mode,
        "eaasp-goose-runtime starting"
    );

    let adapter = Arc::new(GooseAdapter::with_mode(&deployment_mode));
    let service = GooseRuntimeService::new(adapter, &deployment_mode);

    Server::builder()
        .add_service(RuntimeServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
