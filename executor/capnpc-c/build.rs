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

    env::set_current_dir(&out_dir).unwrap();
    assert!(process::Command::new("cmake")
        .args([src_dir.as_os_str()])
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

    let bin = out_dir.join("capnpc-c").canonicalize().unwrap();

    fs::copy(
        bin,
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
}
