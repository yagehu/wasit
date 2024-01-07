use std::{env, fs, path::PathBuf, process};

fn main() {
    println!("cargo:rerun-if-changed=../wazzi-executor.capnp");

    let target_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..")
        .join("target")
        .join(env::var("PROFILE").unwrap());
    let c_capnproto_path = PathBuf::from("..")
        .join("c-capnproto")
        .canonicalize()
        .unwrap();
    let schema_path = PathBuf::from("..")
        .join("wazzi-executor.capnp")
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .canonicalize()
        .unwrap();

    assert!(process::Command::new("capnp")
        .args([
            "compile",
            "-o",
            &format!(
                "{}:{}",
                target_dir
                    .join("capnpc-c")
                    .canonicalize()
                    .unwrap()
                    .display(),
                out_dir.display()
            ),
            &format!("--src-prefix={}", schema_path.parent().unwrap().display()),
            &schema_path.to_string_lossy().to_string(),
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

    cc::Build::new()
        .file(out_dir.join("wazzi-executor.capnp.c"))
        .include(c_capnproto_path.join("lib"))
        .compile("wazzi_executor_capnp");
    fs::copy(
        out_dir.join("libwazzi_executor_capnp.a"),
        target_dir.join("libwazzi_executor_capnp.a"),
    )
    .unwrap();
}
