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

    let protobuf_install_dir = target_dir.join("protobuf");
    let status = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap())
        .env(
            "protobuf_CFLAGS",
            format!("-I{}", protobuf_install_dir.join("include").display()),
        )
        .env(
            "PKG_CONFIG_PATH",
            protobuf_install_dir.join("lib").join("pkgconfig"),
        )
        .env("PROTOC", protobuf_install_dir.join("bin").join("protoc"))
        .env("LDFLAGS", "-framework CoreFoundation")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("make")
        .arg("-j8")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
