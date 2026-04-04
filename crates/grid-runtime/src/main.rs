//! grid-runtime — EAASP L1 gRPC server entry point.
//!
//! Starts a gRPC server exposing the 13-method RuntimeContract
//! for the EAASP platform to manage.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "grid_runtime=info".into()),
        )
        .init();

    tracing::info!("grid-runtime starting (EAASP L1 Tier 1 Harness)");

    // TODO(W3): Initialize GridHarness + start gRPC server
    tracing::info!("gRPC server not yet implemented — see W3 in design plan");

    Ok(())
}
