use std::{
    env,
    fs,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ntest::timeout;
use tempfile::tempdir;

use wazzi_wasi::ProgSeed;

extern crate wazzi_witx as witx;

fn repo_root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), ".."].into_iter().collect()
}

fn spec() -> witx::Document {
    let dir = repo_root().join("spec").join("preview1").join("witx");

    witx::load(&[
        dir.join("typenames.witx"),
        dir.join("wasi_snapshot_preview1.witx"),
    ])
    .unwrap()
}

fn executor_bin() -> PathBuf {
    let profile = std::env!("OUT_DIR")
        .split(std::path::MAIN_SEPARATOR)
        .nth_back(3)
        .unwrap_or_else(|| "unknown")
        .to_string();

    repo_root()
        .join("target")
        .join(&profile)
        .join("wazzi-executor.wasm")
        .canonicalize()
        .unwrap()
}

#[test]
#[timeout(500)]
fn creat() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "00-creat.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    assert!(
        seed.execute(&mut executor, &spec).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let stderr_str = String::from_utf8(stderr.try_lock().unwrap().deref().clone()).unwrap();

    base_dir.path().join("a").canonicalize().expect(&format!(
        "00-creat seed should create file `a`\nexecutor stderr:\n{}\n",
        stderr_str,
    ));
}
