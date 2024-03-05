extern crate wazzi_witx as witx;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use color_eyre::eyre::{self, Context as _};
use wazzi_executor::ExecutorRunner;
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
    let results_root = PathBuf::from("abc");

    thread::scope(|scope| -> Result<_, eyre::Error> {
        let wamr = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            move || -> Result<(), eyre::Error> {
                let result_dir = results_root.join("wamr");
                let base_dir = result_dir.join("base");

                fs::create_dir(&result_dir)?;
                fs::create_dir(&base_dir)?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let wamr = wazzi_runners::Wamr::new("iwasm");
                let executor = ExecutorRunner::new(wamr, executor_bin(), Some(base_dir))
                    .run(stderr.clone())
                    .expect("failed to run executor");
                let store = ExecutionStore::new(
                    &results_root.join("wamr"),
                    &get_commit_id(&PathBuf::from("runtimes").join("wasm-micro-runtime"))?,
                    executor.pid(),
                )?;

                let prog = seed
                    .execute(&spec(), store, executor.clone())
                    .wrap_err("failed to execute seed")?;

                executor.kill();
                prog.into_store().into_path();

                Ok(())
            }
        });
        let wasmedge = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            move || -> Result<(), eyre::Error> {
                let result_dir = results_root.join("wasmedge");
                let base_dir = result_dir.join("base");

                fs::create_dir(&result_dir)?;
                fs::create_dir(&base_dir)?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let runtime = wazzi_runners::Wasmedge::new("wasmedge");
                let executor = ExecutorRunner::new(runtime, executor_bin(), Some(base_dir))
                    .run(stderr.clone())
                    .expect("failed to run executor");
                let store = ExecutionStore::new(
                    &results_root.join("wasmedge"),
                    &get_commit_id(&PathBuf::from("runtimes").join("WasmEdge"))?,
                    executor.pid(),
                )?;

                let prog = seed
                    .execute(&spec(), store, executor.clone())
                    .wrap_err("failed to execute seed")?;

                executor.kill();
                prog.into_store().into_path();

                Ok(())
            }
        });
        let wasmer = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            move || -> Result<(), eyre::Error> {
                let result_dir = results_root.join("wasmer");
                let base_dir = result_dir.join("base");

                fs::create_dir(&result_dir)?;
                fs::create_dir(&base_dir)?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let runtime = wazzi_runners::Wasmer::new("wasmer");
                let executor = ExecutorRunner::new(runtime, executor_bin(), Some(base_dir))
                    .run(stderr.clone())
                    .expect("failed to run executor");
                let store = ExecutionStore::new(
                    &results_root.join("wasmer"),
                    &get_commit_id(&PathBuf::from("runtimes").join("wasmer"))?,
                    executor.pid(),
                )?;
                let prog = seed
                    .execute(&spec(), store, executor.clone())
                    .wrap_err("failed to execute seed")?;

                executor.kill();
                prog.into_store().into_path();

                Ok(())
            }
        });
        let wasmtime = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();

            move || -> Result<(), eyre::Error> {
                let result_dir = results_root.join("wasmtime");
                let base_dir = result_dir.join("base");

                fs::create_dir(&result_dir)?;
                fs::create_dir(&base_dir)?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
                let executor = ExecutorRunner::new(wasmtime, executor_bin(), Some(base_dir))
                    .run(stderr.clone())
                    .expect("failed to run executor");
                let store = ExecutionStore::new(
                    &results_root.join("wasmtime"),
                    &get_commit_id(&PathBuf::from("runtimes").join("wasmtime"))?,
                    executor.pid(),
                )?;

                let prog = seed
                    .execute(&spec(), store, executor.clone())
                    .wrap_err("failed to execute seed")?;

                executor.kill();
                prog.into_store().into_path();

                Ok(())
            }
        });

        Ok((
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
