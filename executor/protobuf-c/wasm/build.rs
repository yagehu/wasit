use std::{env, fs, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root
        .join("executor")
        .join("protobuf-c")
        .join("upstream")
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    env::set_current_dir(&upstream_dir).unwrap();

    assert!(process::Command::new("./autogen.sh")
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    let wasi_sdk = PathBuf::from(env::var("WASI_SDK").unwrap());
    let wasi_sdk_bin_dir = wasi_sdk.join("bin").canonicalize().unwrap();
    let mut protobuf = None;

    if let Ok(p) = env::var("PROTOBUF") {
        protobuf = Some(PathBuf::from(p));
    }

    let protoc = PathBuf::from("protoc");

    env::set_current_dir(&out_dir).unwrap();

    let mut cmd = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap());

    cmd.args(["--host=wasm32-wasi"]);

    if let Some(protobuf) = protobuf {
        cmd.env(
            "protobuf_CFLAGS",
            format!("-I{}", protobuf.join("include").display()),
        )
        .env(
            "protobuf_LIBS",
            format!("-lprotobuf -L{}", protobuf.join("lib").display()),
        )
        .env(
            "PKG_CONFIG_PATH",
            format!(
                "{}:{}",
                protobuf.join("lib").join("pkgconfig").display(),
                protobuf.join("lib64").join("pkgconfig").display(),
            ),
        );
    }

    let status = cmd
        .env("PROTOC", &protoc)
        .env("CC", &wasi_sdk_bin_dir.join("clang"))
        .env("AR", &wasi_sdk_bin_dir.join("ar"))
        .env("LD", &wasi_sdk_bin_dir.join("wasm-ld"))
        .env("NM", &wasi_sdk_bin_dir.join("nm"))
        .env("RANLIB", &wasi_sdk_bin_dir.join("ranlib"))
        .env(
            "CFLAGS",
            format!(
                "--sysroot={}",
                wasi_sdk
                    .join("share")
                    .join("wasi-sysroot")
                    .canonicalize()
                    .unwrap()
                    .display(),
            ),
        )
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let archive_file_relpath = PathBuf::from("protobuf-c").join("libprotobuf-c.la");
    let status = process::Command::new("make")
        .arg(&archive_file_relpath)
        .arg("-j4")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let target_dir = root
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();

    fs::copy(
        out_dir
            .join("protobuf-c")
            .join(".libs")
            .join("libprotobuf-c.a"),
        target_dir.join("libprotobuf-c.a"),
    )
    .unwrap();
}
