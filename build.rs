fn main() {
    tonic_build::compile_protos("proto/msg_hub.proto")
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
