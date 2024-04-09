use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
        Arc,
        Condvar,
        Mutex,
    },
    thread,
    time::Duration,
};

use arbitrary::Unstructured;
use color_eyre::eyre::{self, eyre as err, Context};
use rand::{thread_rng, RngCore};
use tracing::info;
use walkdir::WalkDir;
use wazzi_executor::ExecutorRunner;
use wazzi_runners::WasiRunner;
use wazzi_spec::parsers::Span;
use wazzi_store::FuzzStore;
use wazzi_wazi::seed::Seed;

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
        store: &mut FuzzStore,
        initial_data: Option<&[u8]>,
    ) -> Result<(), eyre::Error> {
        let main_thread = thread::current();
        let data = match initial_data {
            | Some(initial_data) => initial_data.to_vec(),
            | None => {
                let mut data = vec![0u8; 1048576];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };
        let mut run_store = store.new_run().wrap_err("failed to init run store")?;

        run_store
            .write_data(&data)
            .wrap_err("failed to write data")?;

        let spec = fs::read_to_string(PathBuf::from("spec").join("preview1.witx"))
            .wrap_err("failed to read spec to string")?;
        let doc = wazzi_spec::parsers::wazzi_preview1::Document::parse(Span::new(&spec))
            .map_err(|_| err!("failed to parse spec"))?;
        let spec = doc.into_package().wrap_err("failed to process spec")?;

        thread::scope(|scope| -> Result<(), eyre::Error> {
            let mut threads = Vec::with_capacity(self.runtimes.len());
            let n_live_threads = Arc::new(AtomicUsize::new(self.runtimes.len()));
            let pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let cancel = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::channel();

            for runtime in &self.runtimes {
                let runtime_store = run_store
                    .new_runtime(
                        runtime.name.to_owned(),
                        &get_commit_id(&runtime.repo).unwrap(),
                    )
                    .wrap_err("failed to init runtime store")?;

                threads.push((
                    runtime.clone(),
                    thread::Builder::new()
                        .name(runtime.name.to_owned())
                        .spawn_scoped(scope, {
                            let n_live_threads = n_live_threads.clone();
                            let pair = pair.clone();
                            let main_thread = main_thread.clone();
                            let seed = self.seed.clone();
                            let spec = spec.clone();
                            let mut u = Unstructured::new(&data);
                            let tx = tx.clone();

                            move || -> Result<(), eyre::Error> {
                                let wake_main = |err| {
                                    main_thread.unpark();

                                    err
                                };
                                let stderr_file = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(&runtime_store.path.join("stderr"))
                                    .wrap_err("failed to open stderr file")
                                    .map_err(wake_main)?;
                                let executor = ExecutorRunner::new(
                                    runtime.runner.as_ref(),
                                    executor_bin(),
                                    runtime_store.path.clone(),
                                    Some(runtime_store.base.clone()),
                                )
                                .run(Arc::new(Mutex::new(stderr_file)))
                                .wrap_err("failed to run executor")
                                .map_err(wake_main)?;
                                let mut prog = seed
                                    .clone()
                                    .execute(&spec, executor, runtime_store)
                                    .wrap_err("failed to execute seed")
                                    .map_err(wake_main)?;

                                tx.send(()).wrap_err("failed to send done to differ")?;

                                loop {
                                    prog.call_arbitrary(&mut u, &spec)?;

                                    let (mu, cond) = &*pair;
                                    let mut ct = mu.lock().unwrap();
                                    let generation = ct.1;

                                    ct.0 += 1;

                                    if ct.0 == n_live_threads.load(Ordering::SeqCst) {
                                        ct.0 = 0;
                                        ct.1 = ct.1.wrapping_add(1);
                                        tx.send(()).wrap_err("failed to send done to differ")?;
                                        cond.notify_all();
                                    }

                                    let _state = cond
                                        .wait_while(ct, |(_n, gen)| generation == *gen)
                                        .unwrap();
                                }
                            }
                        })
                        .expect("failed to spawn thread"),
                ));
            }

            let t = thread::Builder::new()
                .name("differ".to_owned())
                .spawn_scoped(scope, {
                    let runtimes = run_store
                        .runtimes()
                        .wrap_err("failed to resume runtimes")?
                        .collect::<Vec<_>>();
                    let cancel = cancel.clone();

                    move || -> Result<(), eyre::Error> {
                        while let Ok(_done) = rx.recv() {
                            'outer: for (i, runtime_0) in runtimes.iter().enumerate() {
                                if i == runtimes.len() - 1 {
                                    break;
                                }

                                for j in (i + 1)..runtimes.len() {
                                    let runtime_1 = runtimes.get(j).unwrap();
                                    let runtime_0_walk =
                                        WalkDir::new(&runtime_0.base).sort_by_file_name();
                                    let runtime_1_walk =
                                        WalkDir::new(&runtime_1.base).sort_by_file_name();

                                    for (a, b) in
                                        runtime_0_walk.into_iter().zip(runtime_1_walk.into_iter())
                                    {
                                        let a = a.wrap_err("failed to read dir entry")?;
                                        let b = b.wrap_err("failed to read dir entry")?;

                                        if a.depth() != b.depth()
                                            || a.file_type() != b.file_type()
                                            || a.file_name() != b.file_name()
                                            || (a.file_type().is_file()
                                                && fs::read(a.path())? != fs::read(b.path())?)
                                        {
                                            tracing::warn!(
                                                runtime_0 = %runtime_0.base.display(),
                                                runtime_1 = %runtime_1.base.display(),
                                                "Fs diff found."
                                            );
                                            cancel.store(true, Ordering::SeqCst);
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }

                        panic!("differ thread exit");
                    }
                })
                .wrap_err("failed to spawn differ thread")?;

            // Loop until some thread finishes.
            loop {
                let mut finished = Vec::new();

                for (i, t) in threads.iter().enumerate() {
                    if t.1.is_finished() {
                        finished.push(i);
                    }
                }

                n_live_threads.fetch_sub(finished.len(), Ordering::SeqCst);

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
