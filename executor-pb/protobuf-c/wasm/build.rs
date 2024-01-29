use std::{env, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root
        .join("executor-pb")
        .join("protobuf-c")
        .join("upstream")
        .canonicalize()
        .unwrap();

    println!("cargo:rerun-if-changed={}", upstream_dir.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    env::set_current_dir(&upstream_dir).unwrap();

    let status = process::Command::new("./autogen.sh")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let configure_script = upstream_dir.join("configure").canonicalize().unwrap();
    let wasi_sdk_build_dir = root
        .join("wasi-sdk")
        .join("upstream")
        .join("build")
        .canonicalize()
        .unwrap();
    let wasi_sdk_bin_dir = wasi_sdk_build_dir
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("bin")
        .canonicalize()
        .unwrap();
    let clang = wasi_sdk_bin_dir.join("clang").canonicalize().unwrap();

    env::set_current_dir(&out_dir).unwrap();

    let status = process::Command::new(&configure_script)
        .args(["--host", "wasm32-wasi"])
        .env("CC", &clang)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("make")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
