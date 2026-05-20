fn main() {
    // Compile the Pro-tier proto package alongside the standard Tauri build.
    // The .proto lives under `proto/` in this crate (vendored from
    // `nodespace-sync/nodespaced-pro/proto/`). When sync is checked out
    // as a sibling, `scripts/refresh-pro-proto.sh` re-vendors from the
    // source-of-truth.
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .expect("protoc-bin-vendored is required for the Pro proto build");
    std::env::set_var("PROTOC", &protoc);
    tonic_build::configure()
        .build_server(false) // Tauri client only; daemon defines the server.
        .compile_protos(&["proto/nodespace_pro.proto"], &["proto"])
        .expect("failed to compile nodespace.pro.v1 proto");

    println!("cargo:rerun-if-changed=proto/nodespace_pro.proto");

    tauri_build::build()
}
