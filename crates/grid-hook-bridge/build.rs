fn main() -> Result<(), Box<dyn std::error::Error>> {
    // EAASP v2.0 — hook.proto depends on common.proto and runtime.proto
    // (for HookEventType). All three live in package `eaasp.runtime.v2`.
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/eaasp/runtime/v2/common.proto",
                "../../proto/eaasp/runtime/v2/runtime.proto",
                "../../proto/eaasp/runtime/v2/hook.proto",
            ],
            &["../../proto"],
        )?;

    Ok(())
}
