use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=main.c");
    println!("cargo:rerun-if-changed=stb_ds.h");
    println!("cargo:rerun-if-changed=wasi_snapshot_preview1.h");

    let root_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let target_dir = root_dir.join("target").join(env::var("PROFILE").unwrap());
    let c_capnproto_dir = root_dir
        .join("executor")
        .join("c-capnproto")
        .canonicalize()
        .unwrap();
    let clang_path = root_dir
        .join("wasi-sdk")
        .join("build")
        .join("install")
        .join("opt")
        .join("wasi-sdk")
        .join("bin")
        .join("clang")
        .canonicalize()
        .unwrap();

    cc::Build::new()
        .files(["main.c"])
        .include(target_dir)
        .include(c_capnproto_dir.join("lib"))
        .compiler(clang_path)
        .target("wasm32-wasi")
        .compile("wazzi-executor");

    // let cmake_path = PathBuf::from("CMakeLists.txt").canonicalize().unwrap();
    // let wasi_sdk_cmake_path = [
    //     "..",
    //     "wasi-sdk",
    //     "build",
    //     "install",
    //     "opt",
    //     "wasi-sdk",
    //     "share",
    //     "cmake",
    //     "wasi-sdk.cmake",
    // ]
    // .into_iter()
    // .collect::<PathBuf>()
    // .canonicalize()
    // .unwrap();
    // let dir = env::current_dir().unwrap();
    // let build_dir = [&env::var("OUT_DIR").unwrap(), "build"]
    //     .into_iter()
    //     .collect::<PathBuf>();

    // fs::create_dir_all(&build_dir).unwrap();
    // env::set_current_dir(&build_dir).unwrap();

    // // First, build `capnpc-c`.
    // assert!(process::Command::new("cmake")
    //     .args(vec![cmake_path.to_string_lossy().to_string()])
    //     .spawn()
    //     .unwrap()
    //     .wait()
    //     .unwrap()
    //     .success());
    // assert!(process::Command::new("cmake")
    //     .args(vec!["--build", ".", "--target", "capnpc-c"])
    //     .spawn()
    //     .unwrap()
    //     .wait()
    //     .unwrap()
    //     .success());
    // assert!(process::Command::new("cmake")
    //     .args(vec!["--install", ".", "--component", "capnpc-c"])
    //     .spawn()
    //     .unwrap()
    //     .wait()
    //     .unwrap()
    //     .success());

    // assert!(process::Command::new("cmake")
    //     .args(vec![
    //         cmake_path.to_string_lossy().to_string(),
    //         format!("-DCMAKE_TOOLCHAIN_FILE={}", wasi_sdk_cmake_path.display()),
    //         "-DBUILD_TESTING=OFF".to_owned(),
    //     ])
    //     .spawn()
    //     .unwrap()
    //     .wait()
    //     .unwrap()
    //     .success());
    // // Run capnpc to generate C files.
    // env::set_current_dir(&dir).unwrap();
    // capnpc::CompilerCommand::new()
    //     .file("wazzi-executor.capnp")
    //     .run()
    //     .expect("schema compiler command");
    // env::set_current_dir(&build_dir).unwrap();

    // assert!(process::Command::new("cmake")
    //     .args(vec!["--build", "."])
    //     .spawn()
    //     .unwrap()
    //     .wait()
    //     .unwrap()
    //     .success());

    // let executor_bin = build_dir.join("wazzi-executor").canonicalize().unwrap();

    // fs::copy(
    //     executor_bin,
    //     [
    //         env::var("CARGO_MANIFEST_DIR").unwrap(),
    //         "..".to_owned(),
    //         "target".to_owned(),
    //         env::var("PROFILE").unwrap(),
    //         "wazzi-executor.wasm".to_owned(),
    //     ]
    //     .into_iter()
    //     .collect::<PathBuf>(),
    // )
    // .unwrap();
}
