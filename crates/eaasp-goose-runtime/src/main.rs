// eaasp-goose-runtime stub entry point.
//
// W1.T2.5 scope: read EAASP_DEPLOYMENT_MODE env var (per ADR-V2-019 D2) and log on startup.
// W1.T3 will wire the tonic gRPC server + GooseAdapter::with_mode() plumbing.
fn main() {
    let deployment_mode = std::env::var("EAASP_DEPLOYMENT_MODE")
        .unwrap_or_else(|_| "shared".to_string());
    tracing_subscriber::fmt::init();
    tracing::info!(
        mode = %deployment_mode,
        "eaasp-goose-runtime stub — gRPC server wiring lands in W1.T3"
    );
    // TODO(W1.T3): build tonic server, pass deployment_mode into GooseAdapter::with_mode()
    eprintln!("eaasp-goose-runtime: mode={deployment_mode} (stub, no server bind yet)");
}
