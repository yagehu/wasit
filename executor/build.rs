use std::{env, fs, path::PathBuf, process};

fn main() {
    println!("cargo:rerun-if-changed=CMakeLists.txt");
    println!("cargo:rerun-if-changed=wazzi-executor.capnp");
    println!("cargo:rerun-if-changed=main.c");

    let cmake_path = PathBuf::from("CMakeLists.txt").canonicalize().unwrap();
    let wasi_sdk_cmake_path = [
        "..",
        "wasi-sdk",
        "build",
        "install",
        "opt",
        "wasi-sdk",
        "share",
        "cmake",
        "wasi-sdk.cmake",
    ]
    .into_iter()
    .collect::<PathBuf>()
    .canonicalize()
    .unwrap();
    let dir = env::current_dir().unwrap();
    let build_dir = [&env::var("OUT_DIR").unwrap(), "build"]
        .into_iter()
        .collect::<PathBuf>();

    fs::create_dir_all(&build_dir).unwrap();
    env::set_current_dir(&build_dir).unwrap();

    // CMake configure.
    assert!(process::Command::new("cmake")
        .args(vec![
            cmake_path.to_string_lossy().to_string(),
            format!("-DCMAKE_TOOLCHAIN_FILE={}", wasi_sdk_cmake_path.display()),
            "-DBUILD_TESTING=OFF".to_owned(),
        ])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    // First, build `capnpc-c`.
    assert!(process::Command::new("cmake")
        .args(vec!["--build", ".", "--target", "capnpc-c"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
    assert!(process::Command::new("cmake")
        .args(vec!["--install", ".", "--component", "capnpc-c"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    // Run capnpc to generate C files.
    env::set_current_dir(&dir).unwrap();
    capnpc::CompilerCommand::new()
        .file("wazzi-executor.capnp")
        .run()
        .expect("schema compiler command");
    env::set_current_dir(&build_dir).unwrap();

    assert!(process::Command::new("cmake")
        .args(vec!["--build", "."])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());

    let executor_bin = build_dir.join("wazzi-executor").canonicalize().unwrap();

    fs::copy(
        executor_bin,
        [
            env::var("CARGO_MANIFEST_DIR").unwrap(),
            "..".to_owned(),
            "target".to_owned(),
            env::var("PROFILE").unwrap(),
            "wazzi-executor.wasm".to_owned(),
        ]
        .into_iter()
        .collect::<PathBuf>(),
    )
    .unwrap();
}
