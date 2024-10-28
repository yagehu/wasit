use std::{env, path::PathBuf, process};

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

    env::set_current_dir(&out_dir).unwrap();

    #[cfg(feature = "build-protobuf")]
    let protobuf_install_dir = target_dir.join("protobuf");

    #[cfg(feature = "build-protobuf")]
    let protoc = protobuf_install_dir.join("bin").join("protoc");
    #[cfg(not(feature = "build-protobuf"))]
    let protoc = PathBuf::from("protoc");

    let mut command = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap());

    #[cfg(feature = "build-protobuf")]
    command
        .env(
            "protobuf_CFLAGS",
            format!("-I{}", protobuf_install_dir.join("include").display()),
        )
        .env(
            "PKG_CONFIG_PATH",
            format!(
                "{}:{}",
                protobuf_install_dir.join("lib").join("pkgconfig").display(),
                protobuf_install_dir
                    .join("lib64")
                    .join("pkgconfig")
                    .display(),
            ),
        );

    #[cfg(target_os = "macos")]
    command.env("LDFLAGS", "-framework CoreFoundation");

    let status = command
        .env("PROTOC", &protoc)
        .arg("--prefix")
        .arg(target_dir.join("protoc-c"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("make")
        .args(&["-j8", "install"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
