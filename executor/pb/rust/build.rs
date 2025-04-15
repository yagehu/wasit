use wazzi_compile_time::root;

fn main() {
    let root = root();
    let schema_dir = root.join("executor");
    let schema_file = schema_dir.join("wazzi-executor.proto");

    println!("cargo::rerun-if-changed={}", schema_file.display());

    let mut protoc = protobuf_codegen::Codegen::new();

    protoc.protoc();
    protoc
        .includes([&schema_dir])
        .input(&schema_file)
        .cargo_out_dir("pb-gen")
        .run_from_script();
}
