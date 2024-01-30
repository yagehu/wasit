use std::path::PathBuf;

fn main() {
    let schema_dir = PathBuf::from("..").join("..").canonicalize().unwrap();
    let schema_file = schema_dir
        .join("wazzi-executor.proto")
        .canonicalize()
        .unwrap();

    println!("cargo:rerun-if-changed={}", schema_file.display());

    protobuf_codegen::Codegen::new()
        .protoc()
        .includes([&schema_dir])
        .input(&schema_file)
        .cargo_out_dir("pb-gen")
        .run_from_script();
}
