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
use itertools::{EitherOrBoth, Itertools};
use rand::{thread_rng, RngCore};
use walkdir::WalkDir;
use wazzi_executor::ExecutorRunner;
use wazzi_runners::WasiRunner;
use wazzi_spec::{package::TypeidxBorrow, parsers::Span};
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
            let pause_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let resume_pair = Arc::new((Mutex::new((false, 0usize)), Condvar::new()));
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
                            let pause_pair = pause_pair.clone();
                            let resume_pair = resume_pair.clone();
                            let main_thread = main_thread.clone();
                            let seed = self.seed.clone();
                            let spec = spec.clone();
                            let mut u = Unstructured::new(&data);
                            let tx = tx.clone();
                            let cancel = cancel.clone();

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
                                let (pause_mu, pause_cond) = &*pause_pair;
                                let mut pause_state = pause_mu.lock().unwrap();
                                let pause_generation = pause_state.1;

                                pause_state.0 += 1;

                                if pause_state.0 == n_live_threads.load(Ordering::SeqCst) {
                                    pause_state.0 = 0;
                                    pause_state.1 = pause_state.1.wrapping_add(1);
                                    tx.send(()).wrap_err("failed to send done to differ")?;
                                    pause_cond.notify_all();
                                }

                                let pause_state = pause_cond
                                    .wait_while(pause_state, |(n, generation)| {
                                        *n != n_live_threads.load(Ordering::SeqCst)
                                            && pause_generation == *generation
                                    })
                                    .unwrap();

                                drop(pause_state);

                                loop {
                                    let (resume_mu, resume_cond) = &*resume_pair;
                                    let resume_state = resume_mu.lock().unwrap();
                                    let resume_generation = resume_state.1;
                                    let resume_state = resume_cond
                                        .wait_while(resume_state, |(resume, generation)| {
                                            !*resume && *generation == resume_generation
                                        })
                                        .unwrap();

                                    drop(resume_state);

                                    if cancel.load(Ordering::SeqCst) {
                                        return Ok(());
                                    }

                                    prog.call_arbitrary(&mut u, &spec)?;

                                    let (pause_mu, pause_cond) = &*pause_pair;
                                    let mut ct = pause_mu.lock().unwrap();
                                    let generation = ct.1;

                                    ct.0 += 1;

                                    if ct.0 == n_live_threads.load(Ordering::SeqCst) {
                                        resume_mu.lock().unwrap().0 = false;
                                        ct.0 = 0;
                                        ct.1 = ct.1.wrapping_add(1);
                                        tx.send(()).wrap_err("failed to send done to differ")?;
                                        pause_cond.notify_all();
                                    }

                                    let _state = pause_cond
                                        .wait_while(ct, |(_n, gen)| generation == *gen)
                                        .unwrap();

                                    if cancel.load(Ordering::SeqCst) {
                                        return Ok(());
                                    }
                                }
                            }
                        })
                        .expect("failed to spawn thread"),
                ));
            }

            thread::Builder::new()
                .name("differ".to_owned())
                .spawn_scoped(scope, {
                    let cancel = cancel.clone();
                    let spec = fs::read_to_string(PathBuf::from("spec").join("preview1.witx"))
                        .wrap_err("failed to read spec to string")?;
                    let doc =
                        wazzi_spec::parsers::wazzi_preview1::Document::parse(Span::new(&spec))
                            .map_err(|_| err!("failed to parse spec"))?;
                    let spec = doc.into_package().wrap_err("failed to process spec")?;

                    move || -> Result<(), eyre::Error> {
                        let interface = spec
                            .interface(TypeidxBorrow::Symbolic("wasi_snapshot_preview1"))
                            .unwrap();

                        while let Ok(_done) = rx.recv() {
                            let runtimes = run_store
                                .runtimes()
                                .wrap_err("failed to resume runtimes")?
                                .collect::<Vec<_>>();

                            'outer: for (i, runtime_0) in runtimes.iter().enumerate() {
                                let call_0 = runtime_0
                                    .trace()
                                    .last_call()
                                    .wrap_err("failed to get last call")?
                                    .unwrap()
                                    .read_call()
                                    .wrap_err("failed to read action")?
                                    .unwrap();
                                let func_spec = interface.function(&call_0.func).unwrap();

                                for j in (i + 1)..runtimes.len() {
                                    let runtime_1 = runtimes.get(j).unwrap();
                                    let call_1 = runtime_1
                                        .trace()
                                        .last_call()
                                        .wrap_err("failed to get last call")?
                                        .unwrap()
                                        .read_call()
                                        .wrap_err("failed to read action")?
                                        .unwrap();

                                    if let Some(result) = func_spec.results.first() {
                                        if !result.unspecified {
                                            match (call_0.errno, call_1.errno) {
                                                | (None, None) => {},
                                                | (Some(errno_0), Some(errno_1))
                                                    if errno_0 == 0 && errno_1 == 0
                                                        || errno_0 != 0 && errno_1 != 0 => {},
                                                | _ => {
                                                    tracing::warn!(
                                                        runtime_a = runtime_0.name(),
                                                        runtime_b = runtime_1.name(),
                                                        runtime_a_errno = call_0.errno,
                                                        runtime_b_errno = call_1.errno,
                                                        "Errno diff found!"
                                                    );

                                                    cancel.store(true, Ordering::SeqCst);
                                                    break 'outer;
                                                },
                                            }
                                        }
                                    }

                                    let runtime_0_walk = WalkDir::new(&runtime_0.base)
                                        .sort_by_file_name()
                                        .min_depth(1)
                                        .into_iter();
                                    let runtime_1_walk = WalkDir::new(&runtime_1.base)
                                        .sort_by_file_name()
                                        .min_depth(1)
                                        .into_iter();

                                    for pair in runtime_0_walk.zip_longest(runtime_1_walk) {
                                        match pair {
                                            | EitherOrBoth::Both(a, b) => {
                                                let a = a.wrap_err("failed to read dir entry")?;
                                                let b = b.wrap_err("failed to read dir entry")?;

                                                if a.depth() != b.depth()
                                                    || a.file_type() != b.file_type()
                                                    || a.file_name() != b.file_name()
                                                    || (a.file_type().is_file()
                                                        && fs::read(a.path())?
                                                            != fs::read(b.path())?)
                                                {
                                                    tracing::warn!("Fs diff found.");

                                                    cancel.store(true, Ordering::SeqCst);
                                                    break 'outer;
                                                }
                                            },
                                            | EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                                                tracing::warn!("Fs diff found.");

                                                cancel.store(true, Ordering::SeqCst);
                                                break 'outer;
                                            },
                                        }
                                    }
                                }
                            }

                            let (resume_mu, resume_cond) = &*resume_pair;
                            let mut resume_state = resume_mu.lock().unwrap();

                            resume_state.0 = true;
                            resume_state.1 = resume_state.1.wrapping_add(1);
                            resume_cond.notify_all();
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

                    tracing::info!(
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
