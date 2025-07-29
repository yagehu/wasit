use std::{env, fs, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root.join("executor").join("protobuf-c").join("upstream");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = root.join("target").join(env::var("PROFILE").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let install_dir = target_dir.join("protoc-c-wasm");
    let wasi_sdk = PathBuf::from(env::var("WASI_SDK").unwrap());
    let wasi_sdk_bin_dir = wasi_sdk.join("bin");
    let wasi_sdk_cmake_toolchain_file = wasi_sdk.join("share").join("cmake").join("wasi-sdk.cmake");

    env::set_current_dir(&upstream_dir).unwrap();

    match target_os.as_str() {
        | "windows" => {
            assert!(process::Command::new("cmake")
                .args(["-G", "Ninja"])
                .arg("-S")
                .arg(upstream_dir.join("build-cmake"))
                .arg("-B")
                .arg(&out_dir)
                .arg("-DBUILD_PROTOC=OFF")
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .arg(format!("-DCMAKE_PREFIX_PATH={}", target_dir.display()))
                .arg(format!(
                    "-DCMAKE_TOOLCHAIN_FILE={}",
                    wasi_sdk_cmake_toolchain_file.display()
                ))
                .arg(format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()))
                .spawn()
                .unwrap()
                .wait()
                .unwrap()
                .success());
            assert!(process::Command::new("cmake")
                .arg("--build")
                .arg(&out_dir)
                .arg("-j")
                .spawn()
                .unwrap()
                .wait()
                .unwrap()
                .success());
            assert!(process::Command::new("cmake")
                .arg("--build")
                .arg(&out_dir)
                .args(["--target", "install", "-j"])
                .spawn()
                .unwrap()
                .wait()
                .unwrap()
                .success());
        },
        | _ => {
            let mut protobuf = None;

            if let Ok(p) = env::var("PROTOBUF") {
                protobuf = Some(PathBuf::from(p));
            }

            let protoc = PathBuf::from("protoc");

            env::set_current_dir(&out_dir).unwrap();

            let mut cmd = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap());

            cmd.args(["--host=wasm32-wasi"]);

            if let Some(protobuf) = protobuf {
                cmd.env("protobuf_CFLAGS", format!("-I{}", protobuf.join("include").display()))
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

            assert!(cmd
                .env("PROTOC", &protoc)
                .env("CC", wasi_sdk_bin_dir.join("clang"))
                .env("CXX", wasi_sdk_bin_dir.join("clang++"))
                .env("AR", wasi_sdk_bin_dir.join("ar"))
                .env("LD", wasi_sdk_bin_dir.join("wasm-ld"))
                .env("NM", wasi_sdk_bin_dir.join("nm"))
                .env("RANLIB", wasi_sdk_bin_dir.join("ranlib"))
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
                .unwrap()
                .success());

            let archive_file_relpath = PathBuf::from("protobuf-c").join("libprotobuf-c.la");
            let status = process::Command::new("make")
                .arg(&archive_file_relpath)
                .arg("-j4")
                .spawn()
                .unwrap()
                .wait()
                .unwrap();

            assert!(status.success());

            fs::copy(
                out_dir.join("protobuf-c").join(".libs").join("libprotobuf-c.a"),
                target_dir.join("libprotobuf-c.a"),
            )
            .unwrap();
        },
    }
}
