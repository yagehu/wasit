use std::{env, fs, path::PathBuf, process};

fn main() {
    println!("cargo:rerun-if-changed=main.c");
    println!("cargo:rerun-if-changed=stb_ds.h");
    println!("cargo:rerun-if-changed=wasi_snapshot_preview1.h");

    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = root_dir.join("target").join(env::var("PROFILE").unwrap());
    let c_capnproto_dir = root_dir
        .join("executor")
        .join("c-capnproto")
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
        .args(["main.c"])
        .args(["-Wall", "-Werror", "-Wpedantic"])
        .arg("-I")
        .arg(&target_dir)
        .arg("-I")
        .arg(c_capnproto_dir.join("lib"))
        .arg("-lwazzi_executor_capnp")
        .arg("-lCapnC_Runtime")
        .arg("-L")
        .arg(&target_dir)
        .arg("-o")
        .arg(out_dir.join("wazzi-executor.wasm"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    fs::copy(
        out_dir.join("wazzi-executor.wasm"),
        target_dir.join("wazzi-executor.wasm"),
    )
    .unwrap();
}
