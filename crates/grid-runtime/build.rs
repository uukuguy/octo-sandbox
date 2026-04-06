fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Compile common.proto (generates eaasp.common.v1.rs)
    tonic_build::configure()
        .build_server(false)
        .build_client(false)
        .compile_protos(
            &["../../proto/eaasp/common/v1/common.proto"],
            &["../../proto"],
        )?;

    // Step 2: Compile runtime.proto with extern_path mapping to our common_proto module
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .extern_path(".eaasp.common.v1", "crate::common_proto")
        .compile_protos(
            &["../../proto/eaasp/runtime/v1/runtime.proto"],
            &["../../proto"],
        )?;

    Ok(())
}
