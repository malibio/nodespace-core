fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path().unwrap();
    std::env::set_var("PROTOC", protoc);
    tonic_build::compile_protos("proto/node_service.proto")?;
    tonic_build::compile_protos("proto/agent_session_service.proto")?;
    Ok(())
}
