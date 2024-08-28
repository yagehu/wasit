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
    let target_dir = root
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();

    env::set_current_dir(&upstream_dir).unwrap();

    let status = process::Command::new("./autogen.sh")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

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

    env::set_current_dir(&out_dir).unwrap();

    let protobuf_install_dir = target_dir.join("protobuf");
    let status = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap())
        .args(["--host=wasm32-wasi", "--disable-strip"])
        .env(
            "protobuf_CFLAGS",
            format!("-I{}", protobuf_install_dir.join("include").display()),
        )
        .env(
            "protobuf_LIBS",
            format!(
                "-lprotobuf -L{}",
                protobuf_install_dir.join("lib").display()
            ),
        )
        .env("PROTOC", protobuf_install_dir.join("bin").join("protoc"))
        .env("CC", &wasi_sdk_bin_dir.join("clang"))
        .env("AR", &wasi_sdk_bin_dir.join("ar"))
        .env("NM", &wasi_sdk_bin_dir.join("nm"))
        .env("RANLIB", &wasi_sdk_bin_dir.join("ranlib"))
        .env(
            "CFLAGS",
            format!(
                "--sysroot={}",
                wasi_sdk_build_dir
                    .join("install")
                    .join("opt")
                    .join("wasi-sdk")
                    .join("share")
                    .join("wasi-sysroot")
                    .display()
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
    // fs::copy(
    //     out_dir
    //         .join("protobuf-c")
    //         .join(".libs")
    //         .join("libprotobuf-c.la"),
    //     target_dir.join("libprotobuf-c.la"),
    // )
    // .unwrap();
}
