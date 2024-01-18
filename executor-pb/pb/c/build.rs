use std::{env, fs, path::PathBuf, process};

fn main() {
    let schema_dir = PathBuf::from("..").join("..").canonicalize().unwrap();
    let pb_c_dir = schema_dir
        .join("protobuf-c")
        .join("upstream")
        .canonicalize()
        .unwrap();
    let schema_path = schema_dir
        .join("wazzi-executor.proto")
        .canonicalize()
        .unwrap();

    println!("cargo:rerun-if-changed={}", schema_path.display());
    println!("cargo:rerun-if-changed={}", pb_c_dir.display());

    let root_dir = schema_dir.join("..").canonicalize().unwrap();
    let target_dir = root_dir
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let clang_path = root_dir
        .join("wasi-sdk")
        .join("build")
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("bin")
        .join("clang")
        .canonicalize()
        .unwrap();
    let ar_path = root_dir
        .join("wasi-sdk")
        .join("build")
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("bin")
        .join("llvm-ar")
        .canonicalize()
        .unwrap();

    assert!(process::Command::new("protoc")
        .args([
            &format!("--proto_path={}", schema_dir.display()),
            &format!("--c_out={}", out_dir.display()),
            schema_path.to_string_lossy().as_ref(),
        ])
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
        .arg(
            root_dir
                .join("wasi-sdk")
                .join("build")
                .join("install")
                .join("opt")
                .join("wasi-sdk")
                .join("share")
                .join("wasi-sysroot"),
        )
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
