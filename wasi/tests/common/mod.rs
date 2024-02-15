extern crate wazzi_witx as witx;

use color_eyre::eyre;
use std::{
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tempfile::{tempdir, TempDir};

use wazzi_executor::ExecutorRunner;
use wazzi_wasi::{
    prog::seed::{self},
    InMemorySnapshotStore,
    Prog,
    WasiSnapshot,
};

pub fn get_seed(name: &str) -> seed::Prog {
    serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(wazzi_compile_time::root().join("seeds").join(name))
            .unwrap(),
    )
    .unwrap()
}

fn executor_bin() -> PathBuf {
    let profile = env!("OUT_DIR")
        .split(std::path::MAIN_SEPARATOR)
        .nth_back(3)
        .unwrap_or_else(|| "unknown")
        .to_string();

    wazzi_compile_time::root()
        .join("target")
        .join(&profile)
        .join("wazzi-executor-pb.wasm")
        .canonicalize()
        .unwrap()
}

#[derive(Debug)]
pub struct RunInstance<S, E> {
    pub base_dir: TempDir,
    pub store:    S,
    pub result:   Result<Prog, E>,
    pub stderr:   String,
}

pub fn run_seed(name: &str) -> RunInstance<InMemorySnapshotStore<WasiSnapshot>, eyre::Error> {
    let seed = get_seed(name);

    run(seed)
}

pub fn run(seed: seed::Prog) -> RunInstance<InMemorySnapshotStore<WasiSnapshot>, eyre::Error> {
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let executor = Arc::new(
        ExecutorRunner::new(wasmtime, executor_bin(), Some(base_dir.path().to_owned()))
            .run(stderr.clone())
            .expect("failed to run executor"),
    );
    let store = Arc::new(Mutex::new(InMemorySnapshotStore::default()));
    let (tx, rx) = mpsc::channel();

    let executor_ = executor.clone();
    let store_ = store.clone();

    thread::spawn(move || {
        let result = seed.execute(&executor_, &spec(), store_.lock().unwrap().deref_mut());

        tx.send(result).unwrap();
    });

    let result = rx.recv_timeout(Duration::from_millis(6000));

    executor.kill();

    let stderr = String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap();
    let result = match result {
        | Ok(result) => result,
        | Err(err) => panic!("Execution timeout.\nstderr:\n{}\nerr:\n{}", stderr, err),
    };

    RunInstance {
        base_dir,
        store: Arc::into_inner(store).unwrap().into_inner().unwrap(),
        result,
        stderr,
    }
}

pub fn spec() -> witx::Document {
    let dir = wazzi_compile_time::root()
        .join("spec")
        .join("preview1")
        .join("witx");

    witx::load(&[
        dir.join("typenames.witx"),
        dir.join("wasi_snapshot_preview1.witx"),
    ])
    .unwrap()
}
