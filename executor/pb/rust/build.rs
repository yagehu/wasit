use std::env;

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let schema_dir = root.join("executor");
    let schema_file = schema_dir
        .join("wazzi-executor.proto")
        .canonicalize()
        .unwrap();

    println!("cargo::rerun-if-changed={}", schema_file.display());

    let mut protoc = protobuf_codegen::Codegen::new();

    protoc.protoc();

    if cfg!(feature = "build-protobuf") {
        let target_dir = root
            .join("target")
            .join(env::var("PROFILE").unwrap())
            .canonicalize()
            .unwrap();
        let target_protobuf_dir = target_dir.join("protobuf");

        protoc.protoc_path(&target_protobuf_dir.join("bin").join("protoc"));
    }

    protoc
        .includes([&schema_dir])
        .input(&schema_file)
        .cargo_out_dir("pb-gen")
        .run_from_script();
}
