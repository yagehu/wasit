extern crate wazzi_witx as witx;

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use wazzi_executor::ExecutorRunner;
use wazzi_wasi::seed::Seed;

pub fn spec() -> witx::Document {
    let dir = PathBuf::from(".")
        .join("spec")
        .join("preview1")
        .join("witx");

    witx::load(&[
        dir.join("typenames.witx"),
        dir.join("wasi_snapshot_preview1.witx"),
    ])
    .unwrap()
}

fn executor_bin() -> PathBuf {
    PathBuf::from(".")
        .join("target")
        .join("debug")
        .join("wazzi-executor-pb.wasm")
        .canonicalize()
        .unwrap()
}

fn main() {
    let seed: Seed = serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(PathBuf::from("seeds").join("00-creat.json"))
            .unwrap(),
    )
    .unwrap();

    let base_dir = PathBuf::from("abc");
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let executor = ExecutorRunner::new(wasmtime, executor_bin(), Some(PathBuf::from("abc")))
        .run(stderr.clone())
        .expect("failed to run executor");

    seed.execute(&spec(), base_dir, executor).unwrap();
}
