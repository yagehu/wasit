use std::{env, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root
        .join("executor")
        .join("protobuf")
        .join("upstream")
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = root
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .canonicalize()
        .unwrap();
    let target_protobuf_dir = target_dir.join("protobuf");

    println!("cargo::rerun-if-changed={}", upstream_dir.display());

    env::set_current_dir(&out_dir).unwrap();

    let status = process::Command::new("cmake")
        .arg(&upstream_dir)
        .arg("-DCMAKE_CXX_STANDARD=14")
        .arg(&format!(
            "-DCMAKE_INSTALL_PREFIX={}",
            target_protobuf_dir.display()
        ))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("cmake")
        .args(&[
            "--build", ".", "-j", "8", "--config", "Release", "--target", "install",
        ])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
