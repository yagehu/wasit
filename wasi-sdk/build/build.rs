use std::{env, fs, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let build_root = root
        .join("target")
        .join(env::var("PROFILE").unwrap())
        .join("wasi-sdk");
    let toolchain_dir = build_root.join("toolchain");
    let sysroot_dir = build_root.join("sysroot");
    let install_dir = build_root.join("install");
    let upstream_dir = PathBuf::from("..").join("upstream").canonicalize().unwrap();

    fs::create_dir_all(&toolchain_dir).unwrap();
    fs::create_dir_all(&sysroot_dir).unwrap();
    env::set_current_dir(PathBuf::from("..").join("upstream")).unwrap();

    let status = process::Command::new("cmake")
        .args(&[
            "-G",
            "Ninja",
            "-DWASI_SDK_BUILD_TOOLCHAIN=ON",
            &format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()),
        ])
        .arg("-B")
        .arg(&toolchain_dir)
        .arg("-S")
        .arg(&upstream_dir)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("cmake")
        .arg("--build")
        .arg(&toolchain_dir)
        .args(&["--target", "install"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("cmake")
        .args(&[
            "-G",
            "Ninja",
            "-DCMAKE_C_COMPILER_WORKS=ON",
            "-DCMAKE_CXX_COMPILER_WORKS=ON",
            &format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()),
            &format!(
                "-DCMAKE_TOOLCHAIN_FILE={}",
                install_dir
                    .join("share")
                    .join("cmake")
                    .join("wasi-sdk.cmake")
                    .display()
            ),
        ])
        .arg("-B")
        .arg(&sysroot_dir)
        .arg("-S")
        .arg(&upstream_dir)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());

    let status = process::Command::new("cmake")
        .arg("--build")
        .arg(&sysroot_dir)
        .args(&["--target", "install"])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
