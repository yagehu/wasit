extern crate wazzi_witx as witx;

use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tempfile::{tempdir, TempDir};
use wazzi_executor::ExecutorRunner;
use wazzi_runners::Wasmtime;
use wazzi_wasi::{prog::Prog, seed::Seed, store::ExecutionStore};

pub fn get_seed(name: &str) -> Seed {
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
pub struct RunInstance {
    pub base_dir: TempDir,
    pub prog:     Prog,
    pub stderr:   String,
}

pub fn run_seed(name: &str) -> RunInstance {
    let seed = get_seed(name);

    run(seed)
}

pub fn run(seed: Seed) -> RunInstance {
    let base_dir = tempdir().unwrap();
    let wasmtime = Wasmtime::new(Path::new("wasmtime"));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let executor = ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        base_dir.path().to_path_buf(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let path = base_dir.path().to_owned();
    let store = ExecutionStore::new(&path, "test", executor.pid()).unwrap();
    let run = thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();

        scope.spawn({
            let executor = executor.clone();

            move || {
                let result = seed.execute(&spec(), store, executor);

                tx.send(result).unwrap();
            }
        });

        let result = rx.recv_timeout(Duration::from_millis(60000));
        let prog = match result {
            | Ok(result) => match result {
                | Ok(prog) => prog,
                | Err(err) => {
                    let stderr = String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap();
                    panic!("Failed to execute seed:\nstderr:\n{stderr}\nerr:\n{err}");
                },
            },
            | Err(err) => panic!("Execution timeout or error.\nerr:\n{}", err),
        };

        prog.executor().kill();

        RunInstance {
            base_dir,
            prog,
            stderr: String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
        }
    });

    run
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
