use std::{env, fs, path::PathBuf, process};

fn main() {
    let src_dir = PathBuf::from(".").canonicalize().unwrap();

    println!(
        "cargo::rerun-if-changed={}",
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

    #[cfg(feature = "build-wasi-sdk")]
    let wasi_sdk = root_dir
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .join("wasi-sdk")
        .join("install")
        .canonicalize()
        .unwrap();
    #[cfg(not(feature = "build-wasi-sdk"))]
    let wasi_sdk = PathBuf::from(env::var("WASI_SDK").unwrap());

    let clang = wasi_sdk.join("bin").join("clang").canonicalize().unwrap();
    let mut child = process::Command::new(clang)
        .arg("--sysroot")
        .arg(
            wasi_sdk
                .join("share")
                .join("wasi-sysroot")
                .canonicalize()
                .unwrap(),
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
        .arg(out_dir.join("wazzi-executor.wasm"))
        .spawn()
        .unwrap();

    assert!(child.wait().unwrap().success());

    fs::copy(
        out_dir.join("wazzi-executor.wasm"),
        target_dir.join("wazzi-executor.wasm"),
    )
    .unwrap();
}
