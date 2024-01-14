use std::{env, path::PathBuf, process};

fn main() {
    let schema_path = PathBuf::from("..")
        .join("wazzi-executor.proto")
        .canonicalize()
        .unwrap();

    println!("cargo:rerun-if-changed={}", schema_path.display());

    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap();
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
            &format!("--c_out={}", out_dir.display()),
            schema_path.to_string_lossy().as_ref(),
        ])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    fs::copy(
        out_dir.join("wazzi-executor.capnp.h"),
        target_dir.join("wazzi-executor.capnp.h"),
    )
    .unwrap();
    fs::copy(
        out_dir.join("wazzi-executor.capnp.c"),
        target_dir.join("wazzi-executor.capnp.c"),
    )
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
        .args([out_dir.join("wazzi-executor.capnp.c")])
        .arg("-I")
        .arg(c_capnproto_path.join("lib"))
        .arg("-lwazzi_executor_capnp")
        .arg("-L")
        .arg(&target_dir)
        .arg("-c")
        .arg("-o")
        .arg(out_dir.join("wazzi_executor_capnp.o"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    assert!(process::Command::new(ar_path)
        .arg("r")
        .arg(out_dir.join("libwazzi_executor_capnp.a"))
        .arg(out_dir.join("wazzi_executor_capnp.o"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    fs::copy(
        out_dir.join("libwazzi_executor_capnp.a"),
        target_dir.join("libwazzi_executor_capnp.a"),
    )
    .unwrap();
}
