fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    tonic_build::configure().compile_protos(
        &[
            "proto/node_service.proto",
            "proto/agent_session_service.proto",
        ],
        &["proto"],
    )?;
    Ok(())
}
