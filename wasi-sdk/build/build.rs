use std::{env, path::PathBuf, process};

fn main() {
    env::set_current_dir(PathBuf::from("..").join("upstream")).unwrap();

    let status = process::Command::new("make")
        .env("NINJA_FLAGS", "-v")
        .arg("build")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    assert!(status.success());
}
