use std::{env, path::Path, process};

use wazzi_compile_time::root;

fn main() {
    let root = root();

    env::set_current_dir(root.join("executor").join("protobuf-c").join("upstream")).unwrap();

    assert!(process::Command::new(Path::new(".").join("autogen.sh"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .success());
}
