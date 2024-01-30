use std::{env, fs, path::PathBuf, process};

fn main() {
    let src_dir = PathBuf::from(".").canonicalize().unwrap();

    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("main.c").display()
    );

    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let target_dir = root_dir
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let protobuf_c_dir = root_dir
        .join("executor")
        .join("protobuf-c")
        .join("upstream")
        .canonicalize()
        .unwrap();
    let wasi_sdk_build_dir = root_dir.join("wasi-sdk").join("upstream").join("build");
    let clang_path = wasi_sdk_build_dir
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("bin")
        .join("clang")
        .canonicalize()
        .unwrap();

    assert!(process::Command::new(clang_path)
        .arg("--sysroot")
        .arg(
            wasi_sdk_build_dir
                .join("install")
                .join("opt")
                .join("wasi-sdk")
                .join("share")
                .join("wasi-sysroot"),
        )
        .args([src_dir.join("main.c")])
        .args(["-Wall", "-Werror", "-Wpedantic"])
        .arg("-I")
        .arg(&target_dir)
        .arg("-I")
        .arg(&protobuf_c_dir)
        .args(["-lwazzi-executor-pb", "-lprotobuf-c"])
        .arg("-L")
        .arg(&target_dir)
        .arg("-o")
        .arg(out_dir.join("wazzi-executor-pb.wasm"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    fs::copy(
        out_dir.join("wazzi-executor-pb.wasm"),
        target_dir.join("wazzi-executor-pb.wasm"),
    )
    .unwrap();
}
