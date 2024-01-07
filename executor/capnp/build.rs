fn main() {
    println!("cargo:rerun-if-changed=../wazzi-executor.capnp");

    capnpc::CompilerCommand::new()
        .file("../wazzi-executor.capnp")
        .src_prefix("../")
        .run()
        .expect("schema compiler command");
}
