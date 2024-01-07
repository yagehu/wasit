use std::{env, fs, path::PathBuf, process};

fn main() {
    println!("cargo:rerun-if-changed=../c-capnproto");

    let src_dir = env::current_dir()
        .unwrap()
        .join("..")
        .join("c-capnproto")
        .canonicalize()
        .unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .canonicalize()
        .unwrap();
    let capnpc_c_build_dir = out_dir.join("capnpc-c-build");
    let capnp_runtime_build_dir = out_dir.join("capnp-runtime-build");
    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap();
    let wasi_sdk_cmake_path = root_dir
        .join("wasi-sdk")
        .join("build")
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("share")
        .join("cmake")
        .join("wasi-sdk.cmake")
        .canonicalize()
        .unwrap();

    fs::create_dir_all(&capnpc_c_build_dir).unwrap();
    fs::create_dir_all(&capnp_runtime_build_dir).unwrap();

    env::set_current_dir(&capnpc_c_build_dir).unwrap();
    assert!(process::Command::new("cmake")
        .arg(&src_dir)
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    assert!(process::Command::new("cmake")
        .args(["--build", "."])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    fs::copy(
        capnpc_c_build_dir.join("capnpc-c").canonicalize().unwrap(),
        [
            env::var("CARGO_MANIFEST_DIR").unwrap(),
            "..".to_owned(),
            "..".to_owned(),
            "target".to_owned(),
            env::var("PROFILE").unwrap(),
            "capnpc-c".to_owned(),
        ]
        .into_iter()
        .collect::<PathBuf>(),
    )
    .unwrap();

    env::set_current_dir(&capnp_runtime_build_dir).unwrap();
    assert!(process::Command::new("cmake")
        .arg(&src_dir)
        .arg(format!(
            "-DCMAKE_TOOLCHAIN_FILE={}",
            wasi_sdk_cmake_path.display()
        ))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    assert!(process::Command::new("cmake")
        .args(["--build", ".", "--target", "CapnC_Runtime-Static"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    fs::copy(
        capnp_runtime_build_dir
            .join("libCapnC_Runtime-Static.a")
            .canonicalize()
            .unwrap(),
        [
            env::var("CARGO_MANIFEST_DIR").unwrap(),
            "..".to_owned(),
            "..".to_owned(),
            "target".to_owned(),
            env::var("PROFILE").unwrap(),
            "libCapnC_Runtime.a".to_owned(),
        ]
        .into_iter()
        .collect::<PathBuf>(),
    )
    .unwrap();
}
