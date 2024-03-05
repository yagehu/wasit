extern crate wazzi_witx as witx;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use color_eyre::eyre::{self, Context as _};
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

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let seed: Seed = serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(PathBuf::from("seeds").join("00-creat.json"))
            .unwrap(),
    )
    .unwrap();
    let runtimes_dir = PathBuf::from("runtimes");
    let results_root = PathBuf::from("abc");

    thread::scope(|scope| -> Result<_, eyre::Error> {
        let wasmtime = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();
            let runtimes_dir = runtimes_dir.clone();

            move || -> Result<(), eyre::Error> {
                fs::create_dir(results_root.join("wasmtime"))?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
                let wasmtime_store = RuntimeStore::new(
                    results_root.join("wasmtime"),
                    &runtimes_dir.join("wasmtime"),
                )?;
                let wasmtime_executor = ExecutorRunner::new(
                    wasmtime,
                    executor_bin(),
                    Some(wasmtime_store.root.clone()),
                )
                .run(stderr.clone())
                .expect("failed to run executor");

                seed.execute(
                    &spec(),
                    wasmtime_store.root.join("prog"),
                    wasmtime_executor.clone(),
                )
                .wrap_err("failed to execute seed")?;

                wasmtime_executor.kill();

                Ok(())
            }
        });
        let wamr = scope.spawn({
            let seed = seed.clone();
            let results_root = results_root.clone();
            let runtimes_dir = runtimes_dir.clone();

            move || -> Result<(), eyre::Error> {
                fs::create_dir(results_root.join("wamr"))?;

                let stderr = Arc::new(Mutex::new(Vec::new()));
                let wamr = wazzi_runners::Wamr::new("iwasm");
                let wamr_store = RuntimeStore::new(
                    results_root.join("wamr"),
                    &runtimes_dir.join("wasm-micro-runtime"),
                )?;
                let wamr_executor =
                    ExecutorRunner::new(wamr, executor_bin(), Some(wamr_store.root.clone()))
                        .run(stderr.clone())
                        .expect("failed to run executor");

                seed.execute(&spec(), wamr_store.root.join("prog"), wamr_executor.clone())
                    .wrap_err("failed to execute seed")?;

                wamr_executor.kill();

                Ok(())
            }
        });

        Ok((wasmtime.join().unwrap()?, wamr.join().unwrap()?))
    })?;

    Ok(())
}

#[derive(Clone, Debug)]
pub struct RuntimeStore {
    root: PathBuf,
}

impl RuntimeStore {
    pub fn new(root: PathBuf, repo_path: &Path) -> Result<Self, eyre::Error> {
        let repo = git2::Repository::open(repo_path).wrap_err("failed to open runtime repo")?;
        let head_ref = repo.head().wrap_err("failed to get head reference")?;
        let head_commit = head_ref
            .peel_to_commit()
            .wrap_err("failed to get commit for head ref")?;
        let root = root.canonicalize()?;

        fs::write(root.join("version"), head_commit.id().to_string())?;
        fs::create_dir(root.join("prog"))?;

        Ok(Self { root })
    }

    pub fn prog_path(&self) -> PathBuf {
        self.root.join("prog")
    }
}
