fn main() -> Result<(), Box<dyn std::error::Error>> {
    // EAASP v2.0 proto — client-only for certifier (it drives runtimes under test).
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &[
                "../../proto/eaasp/runtime/v2/common.proto",
                "../../proto/eaasp/runtime/v2/runtime.proto",
            ],
            &["../../proto"],
        )?;

    Ok(())
}
