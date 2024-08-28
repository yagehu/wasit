use std::{env, path::PathBuf};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let schema_dir = PathBuf::from("..").join("..").canonicalize().unwrap();
    let schema_file = schema_dir
        .join("wazzi-executor.proto")
        .canonicalize()
        .unwrap();
    let target_dir = root
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();
    let target_protobuf_dir = target_dir.join("protobuf");

    println!("cargo::rerun-if-changed={}", schema_file.display());

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&target_protobuf_dir.join("bin").join("protoc"))
        .includes([&schema_dir])
        .input(&schema_file)
        .cargo_out_dir("pb-gen")
        .run_from_script();
}
