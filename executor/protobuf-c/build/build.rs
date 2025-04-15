use std::{env, path::PathBuf, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();
    let upstream_dir = root.join("executor").join("protobuf-c").join("upstream");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = root.join("target").join(env::var("PROFILE").unwrap());
    let install_prefix = target_dir.join("protoc-c");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    env::set_current_dir(&upstream_dir).unwrap();

    if target_os == "windows" {
        assert!(process::Command::new("cmake")
            .args(["-S", "build-cmake", "-B"])
            .arg(&out_dir)
            .args([
                "-DCMAKE_BUILD_TYPE=Release",
                &format!("-DCMAKE_INSTALL_PREFIX={}", install_prefix.display()),
            ])
            .arg("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded")
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        assert!(process::Command::new("cmake")
            .arg("--build")
            .arg(&out_dir)
            .args(["--config", "release"])
            .arg("-j")
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        assert!(process::Command::new("cmake")
            .arg("--build")
            .arg(&out_dir)
            .args(["--config", "release"])
            .args(["-j", "--target", "install"])
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());
    } else {
        assert!(process::Command::new("./autogen.sh")
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());

        env::set_current_dir(&out_dir).unwrap();

        let mut protobuf = None;

        if let Ok(p) = env::var("PROTOBUF") {
            protobuf = Some(PathBuf::from(p));
        }

        let protoc = PathBuf::from("protoc");
        let mut command = process::Command::new(upstream_dir.join("configure").canonicalize().unwrap());

        if let Some(protobuf) = protobuf {
            command
                .env("protobuf_CFLAGS", format!("-I{}", protobuf.join("include").display(),))
                .env(
                    "PKG_CONFIG_PATH",
                    format!(
                        "{}:{}",
                        protobuf.join("lib").join("pkgconfig").display(),
                        protobuf.join("lib64").join("pkgconfig").display(),
                    ),
                );
        }

        command.env("CC", "clang");
        command.env("CXX", "clang++");

        #[cfg(target_os = "macos")]
        command.env("LDFLAGS", "-framework CoreFoundation");

        assert!(command
            .env("PROTOC", &protoc)
            .arg("--prefix")
            .arg(target_dir.join("protoc-c"))
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());
        assert!(process::Command::new("make")
            .args(["-j8", "install"])
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
            .success());
    }
}
