fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Compile common.proto (generates eaasp.common.v1.rs)
    tonic_build::configure()
        .build_server(false)
        .build_client(false)
        .compile_protos(
            &["../../proto/eaasp/common/v1/common.proto"],
            &["../../proto"],
        )?;

    // Step 2: Compile hook.proto with extern_path to our common_proto module
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .extern_path(".eaasp.common.v1", "crate::common_proto")
        .compile_protos(
            &["../../proto/eaasp/hook/v1/hook.proto"],
            &["../../proto"],
        )?;

    Ok(())
}
