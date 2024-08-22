use std::{env, fs, os::unix::fs::PermissionsExt, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root
        .join("executor")
        .join("protobuf")
        .join("upstream")
        .canonicalize()
        .unwrap();

    env::set_current_dir(&upstream_dir).unwrap();

    let status = process::Command::new("bazel")
        .args(&["build", ":protoc", ":protobuf"])
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
    let protoc_bin = target_dir.join("protoc");
    let _ = fs::copy(upstream_dir.join("bazel-bin").join("protoc"), &protoc_bin).unwrap();
    let mut perm = fs::metadata(&protoc_bin).unwrap().permissions();

    #[cfg(unix)]
    perm.set_mode(0o775);

    fs::set_permissions(&protoc_bin, perm).unwrap();
}
