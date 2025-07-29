use std::{env, fs, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let schema_dir = PathBuf::from("..").join("..");
    let pb_c_dir = schema_dir.join("protobuf-c").join("upstream");
    let schema_path = schema_dir.join("wazzi-executor.proto");

    println!("cargo:rerun-if-changed={}", schema_path.display());

    let root_dir = root();
    let target_dir = root_dir.join("target").join(env::var("PROFILE").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wasi_sdk = PathBuf::from(env::var("WASI_SDK").unwrap());
    let protoc = PathBuf::from("protoc");
    let clang_path = wasi_sdk.join("bin").join("clang");
    let ar_path = wasi_sdk.join("bin").join("llvm-ar");

    let protoc_c_plugin_filename = if cfg!(windows) {
        "protoc-gen-c.exe"
    } else {
        "protoc-gen-c"
    };

    assert!(process::Command::new(&protoc)
        .args([
            &format!("--proto_path={}", schema_dir.display()),
            &format!("--c_out={}", out_dir.display()),
            schema_path.to_string_lossy().as_ref(),
        ])
        .arg(format!(
            "--plugin=protoc-gen-c={}",
            target_dir
                .join("protoc-c")
                .join("bin")
                .join(protoc_c_plugin_filename)
                .display()
        ))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    let pb_file_c = target_dir.join("wazzi-executor.pb-c.c");
    let pb_file_h = target_dir.join("wazzi-executor.pb-c.h");

    fs::copy(out_dir.join("wazzi-executor.pb-c.c"), &pb_file_c).unwrap();
    fs::copy(out_dir.join("wazzi-executor.pb-c.h"), pb_file_h).unwrap();

    assert!(process::Command::new(clang_path)
        .arg("--sysroot")
        .arg(wasi_sdk.join("share").join("wasi-sysroot"))
        .args([pb_file_c])
        .arg("-I")
        .arg(&pb_c_dir)
        .arg("-c")
        .arg("-o")
        .arg(out_dir.join("wazzi-executor-pb.o"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    assert!(process::Command::new(ar_path)
        .arg("r")
        .arg(out_dir.join("libwazzi-executor-pb.a"))
        .arg(out_dir.join("wazzi-executor-pb.o"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    fs::copy(
        out_dir.join("libwazzi-executor-pb.a"),
        target_dir.join("libwazzi-executor-pb.a"),
    )
    .unwrap();
}
