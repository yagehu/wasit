extern crate wazzi_witx as witx;

pub mod store;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use arbitrary::Unstructured;
use color_eyre::eyre::{self, Context};
use rand::{thread_rng, RngCore};
use tracing::info;
use wazzi_executor::ExecutorRunner;
use wazzi_runners::WasiRunner;
use wazzi_wasi::{seed::Seed, store::ExecutionStore};

use crate::store::FuzzStore;

#[derive(Clone, Debug)]
pub struct Fuzzer {
    seed:     Seed,
    runtimes: Vec<Runtime>,
}

impl Fuzzer {
    pub fn new(seed: Seed, runtimes: impl IntoIterator<Item = Runtime>) -> Self {
        Self {
            seed,
            runtimes: runtimes.into_iter().collect(),
        }
    }

    pub fn fuzz_loop(
        &mut self,
        store: &FuzzStore,
        initial_data: Option<&[u8]>,
    ) -> Result<(), eyre::Error> {
        let main_thread = thread::current();
        let data = match initial_data {
            | Some(initial_data) => initial_data.to_vec(),
            | None => {
                let mut data = vec![0u8; 65536];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };

        store
            .write_seed_data(&data)
            .wrap_err("failed to write seed data")?;

        thread::scope(|scope| -> Result<(), eyre::Error> {
            let mut threads = Vec::with_capacity(self.runtimes.len());

            for runtime in &self.runtimes {
                threads.push((
                    runtime.clone(),
                    thread::Builder::new()
                        .name(runtime.name.to_owned())
                        .spawn_scoped(scope, {
                            let main_thread = main_thread.clone();
                            let seed = self.seed.clone();
                            let mut u = Unstructured::new(&data);

                            move || -> Result<(), eyre::Error> {
                                let wake_main = |err| {
                                    main_thread.unpark();

                                    err
                                };

                                let result_dir = store.path().join(&runtime.name);
                                let base_dir = result_dir.join("base");

                                fs::create_dir(&result_dir)
                                    .wrap_err("failed to create result dir")
                                    .map_err(wake_main)?;
                                fs::create_dir(&base_dir)
                                    .wrap_err("failed to create base dir")
                                    .map_err(wake_main)?;

                                let stderr_file = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(&result_dir.join("stderr"))
                                    .wrap_err("failed to open stderr file")
                                    .map_err(wake_main)?;
                                let executor = ExecutorRunner::new(
                                    runtime.runner.as_ref(),
                                    executor_bin(),
                                    result_dir.clone(),
                                    Some(base_dir),
                                )
                                .run(Arc::new(Mutex::new(stderr_file)))
                                .wrap_err("failed to run executor")
                                .map_err(wake_main)?;
                                let store = ExecutionStore::new(
                                    &result_dir,
                                    &get_commit_id(&runtime.repo).unwrap(),
                                    executor.pid(),
                                )
                                .wrap_err("failed to initialize execution store")
                                .map_err(wake_main)?;
                                let mut prog = seed
                                    .clone()
                                    .execute(&spec(), store, executor.clone())
                                    .wrap_err("failed to execute seed")
                                    .map_err(wake_main)?;

                                loop {
                                    prog.call_arbitrary(&mut u, &spec()).unwrap();
                                }
                            }
                        })
                        .expect("failed to spawn thread"),
                ));
            }

            // Loop until some thread finishes.
            loop {
                let mut finished = Vec::new();

                for (i, t) in threads.iter().enumerate() {
                    if t.1.is_finished() {
                        finished.push(i);
                    }
                }

                for i in finished.into_iter().rev() {
                    let (_runtime, t) = threads.remove(i);
                    let name = t.thread().name().map(ToOwned::to_owned);
                    let result = t.join().unwrap();

                    info!(
                        result = ?result,
                        "Thread {} finished. {} still running.",
                        name.unwrap_or_else(|| format!("{i}")),
                        threads.len(),
                    );
                }

                if threads.is_empty() {
                    break;
                }

                thread::park_timeout(Duration::from_secs(1));
            }

            Ok(())
        })?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Runtime {
    pub name:   &'static str,
    pub repo:   PathBuf,
    pub runner: Arc<dyn WasiRunner>,
}

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

fn get_commit_id(path: &Path) -> Result<String, eyre::Error> {
    let repo = git2::Repository::open(path).wrap_err("failed to open runtime repo")?;
    let head_ref = repo.head().wrap_err("failed to get head reference")?;
    let head_commit = head_ref
        .peel_to_commit()
        .wrap_err("failed to get commit for head ref")?;

    Ok(head_commit.id().to_string())
}
