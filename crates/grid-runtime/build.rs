fn main() -> Result<(), Box<dyn std::error::Error>> {
    // EAASP v2.0 proto — one tonic_build pass compiles common + runtime together.
    //
    // v2 uses a single package (`eaasp.runtime.v2`) for common and runtime types,
    // so no extern_path trick is needed — all types land in one generated module.
    tonic_build::configure()
        .build_server(true)
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
