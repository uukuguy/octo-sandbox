//! eaasp-claw-code-runtime gRPC server entry point.
//!
//! Listens on CLAW_CODE_RUNTIME_GRPC_ADDR (default 0.0.0.0:50056).

use std::sync::Arc;

use tonic::transport::Server;

use eaasp_claw_code_runtime::adapter::ClawCodeAdapter;
use eaasp_claw_code_runtime::proto::runtime_service_server::RuntimeServiceServer;
use eaasp_claw_code_runtime::service::ClawCodeRuntimeService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let deployment_mode =
        std::env::var("EAASP_DEPLOYMENT_MODE").unwrap_or_else(|_| "shared".to_string());

    let addr = std::env::var("CLAW_CODE_RUNTIME_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50056".to_string())
        .parse()?;

    tracing::info!(addr = %addr, mode = %deployment_mode, "eaasp-claw-code-runtime starting");

    let adapter = Arc::new(ClawCodeAdapter::with_mode(&deployment_mode));
    let service = ClawCodeRuntimeService::new(adapter, &deployment_mode);

    Server::builder()
        .add_service(RuntimeServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
