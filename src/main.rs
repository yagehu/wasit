extern crate wazzi_witx as witx;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use color_eyre::eyre::{self, Context as _};
use wazzi_executor::ExecutorRunner;
use wazzi_runners::{Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime};
use wazzi_wasi::{seed::Seed, store::ExecutionStore};

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

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let seed: Seed = serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(PathBuf::from("seeds").join("00-creat.json"))
            .unwrap(),
    )
    .unwrap();
    let results_root = PathBuf::from("abc").canonicalize()?;

    fn run(
        seed: Seed,
        name: &'static str,
        repo: &'static str,
        results_root: PathBuf,
        runner: impl WasiRunner,
    ) -> impl FnOnce() -> Result<(), eyre::Error> {
        move || {
            let result_dir = results_root.join(name);
            let base_dir = result_dir.join("base");

            fs::create_dir(&result_dir)?;
            fs::create_dir(&base_dir)?;

            let stderr = Arc::new(Mutex::new(Vec::new()));
            let executor =
                ExecutorRunner::new(runner, executor_bin(), result_dir.clone(), Some(base_dir))
                    .run(stderr.clone())
                    .expect("failed to run executor");
            let store = ExecutionStore::new(
                &result_dir,
                &get_commit_id(&PathBuf::from("runtimes").join(repo))?,
                executor.pid(),
            )?;

            let prog = seed
                .execute(&spec(), store, executor.clone())
                .wrap_err("failed to execute seed")?;

            executor.kill();
            prog.into_store().into_path();

            fs::write(result_dir.join("stderr"), &*stderr.lock().unwrap())?;

            Ok(())
        }
    }

    thread::scope(|scope| -> Result<_, eyre::Error> {
        let node = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            run(
                seed,
                "node",
                "node",
                results_root,
                Node::new(Path::new("node")),
            )
        });
        let wamr = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            run(
                seed,
                "wamr",
                "wasm-micro-runtime",
                results_root,
                Wamr::new(Path::new("iwasm")),
            )
        });
        let wasmedge = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            run(
                seed,
                "wasmedge",
                "WasmEdge",
                results_root,
                Wasmedge::new(Path::new("wasmedge")),
            )
        });
        let wasmer = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            run(
                seed,
                "wasmer",
                "wasmer",
                results_root,
                Wasmer::new(Path::new("wasmer")),
            )
        });
        let wasmtime = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            run(
                seed,
                "wasmtime",
                "wasmtime",
                results_root,
                Wasmtime::new(Path::new("wasmtime")),
            )
        });

        Ok((
            node.join().unwrap()?,
            wamr.join().unwrap()?,
            wasmedge.join().unwrap()?,
            wasmer.join().unwrap()?,
            wasmtime.join().unwrap()?,
        ))
    })?;

    Ok(())
}

fn get_commit_id(path: &Path) -> Result<String, eyre::Error> {
    let repo = git2::Repository::open(path).wrap_err("failed to open runtime repo")?;
    let head_ref = repo.head().wrap_err("failed to get head reference")?;
    let head_commit = head_ref
        .peel_to_commit()
        .wrap_err("failed to get commit for head ref")?;

    Ok(head_commit.id().to_string())
}
