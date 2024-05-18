use std::{
    collections::HashMap,
    fs,
    io,
    path::PathBuf,
    sync::{
        atomic::{self, AtomicUsize},
        mpsc,
        Arc,
        Condvar,
        Mutex,
    },
    thread,
};

use arbitrary::Unstructured;
use eyre::{Context, ContextCompat};
use rand::{thread_rng, RngCore as _};
use wazzi_dyn_spec::{ast::Idx, wasi, Environment, Resource, ResourceContext};
use wazzi_executor::ExecutorRunner;
use wazzi_runners::WasiRunner;
use wazzi_store::{FuzzStore, RunStore};

#[derive(Debug)]
pub struct Fuzzer<F> {
    new_env:  F,
    runtimes: Vec<(String, Box<dyn WasiRunner>)>,
    executor: PathBuf,
    store:    FuzzStore,
}

impl<F> Fuzzer<F>
where
    F: Fn() -> Environment + Send + Sync,
{
    pub fn new(
        new_env: F,
        runtimes: impl IntoIterator<Item = (String, Box<dyn WasiRunner>)>,
        executor: PathBuf,
        store: FuzzStore,
    ) -> Self {
        Self {
            new_env,
            runtimes: runtimes.into_iter().collect(),
            executor,
            store,
        }
    }

    pub fn fuzz(&mut self, data: Option<Vec<u8>>) -> Result<(), eyre::Error> {
        let mut scope = FuzzScope::new(self, None)?;

        scope.fuzz()?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct FuzzScope<'f, F> {
    fuzzer: &'f Fuzzer<F>,
    env:    Environment,
    store:  RunStore,
    data:   Vec<u8>,
}

impl<'f, F> FuzzScope<'f, F>
where
    F: Fn() -> Environment + Send + Sync,
{
    pub fn new(fuzzer: &'f mut Fuzzer<F>, data: Option<Vec<u8>>) -> Result<Self, eyre::Error> {
        let mut env = (fuzzer.new_env)();
        let store = fuzzer
            .store
            .new_run()
            .wrap_err("failed to init new run store")?;
        let data = match data {
            | Some(data) => data,
            | None => {
                let mut data = vec![0u8; 1048576];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };

        Ok(Self {
            fuzzer,
            env,
            store,
            data,
        })
    }

    pub fn fuzz(&mut self) -> Result<(), eyre::Error> {
        let data = self.data.clone();

        thread::scope(|scope| -> Result<(), eyre::Error> {
            let pause_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let resume_pair = Arc::new((Mutex::new((false, 0usize)), Condvar::new()));
            let n_live_threads = Arc::new(AtomicUsize::new(self.fuzzer.runtimes.len()));
            let (tx, rx) = mpsc::channel();

            for (name, runtime) in &self.fuzzer.runtimes {
                let mut runtime_store = self
                    .store
                    .new_runtime(name.to_owned(), "")
                    .wrap_err("failed to init new runtime store")?;
                let stderr = Arc::new(Mutex::new(
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&runtime_store.path.join("stderr"))
                        .wrap_err("failed to open stderr file")?,
                ));
                let mut ctx = ResourceContext::new();
                let fd_type_idx = self
                    .env
                    .resource_types()
                    .resolve_idx(&Idx::Symbolic("fd".to_owned()))
                    .wrap_err("no fd resource type")?;

                ctx.push(
                    fd_type_idx,
                    Resource {
                        value: wasi::Value::Handle(runtime.base_dir_fd()),
                        attrs: HashMap::from([
                            (
                                "file-type".to_owned(),
                                wasi::Value::Variant(Box::new(wasi::Variant {
                                    case_idx: 3,
                                    payload:  None,
                                })),
                            ),
                            ("offset".to_owned(), wasi::Value::U64(0)),
                        ]),
                    },
                );

                let t = thread::Builder::new()
                    .name(name.clone())
                    .stack_size(32 * 1024 * 1024)
                    .spawn_scoped(scope, {
                        let executor_path = self.fuzzer.executor.clone();
                        let working_dir = runtime_store.path.clone();
                        let pause_pair = pause_pair.clone();
                        let resume_pair = resume_pair.clone();
                        let n_live_threads = n_live_threads.clone();
                        let tx = tx.clone();
                        let mut u = Unstructured::new(&data);
                        let env = self.env.clone();

                        move || -> Result<(), eyre::Error> {
                            let executor = ExecutorRunner::new(
                                runtime.as_ref(),
                                executor_path,
                                working_dir,
                                Some(runtime_store.base.clone()),
                            )
                            .run(stderr)
                            .wrap_err("failed to run executor")?;

                            let (pause_mu, pause_cond) = &*pause_pair;
                            let mut pause_state = pause_mu.lock().unwrap();
                            let pause_generation = pause_state.1;

                            pause_state.0 += 1;

                            if pause_state.0 == n_live_threads.load(atomic::Ordering::SeqCst) {
                                pause_state.0 = 0;
                                pause_state.1 = pause_state.1.wrapping_add(1);
                                pause_cond.notify_all();
                                tx.send(()).unwrap();
                            }

                            drop(
                                pause_cond
                                    .wait_while(pause_state, |(n, generation)| {
                                        *n != n_live_threads.load(atomic::Ordering::SeqCst)
                                            && pause_generation == *generation
                                    })
                                    .unwrap(),
                            );

                            loop {
                                let (resume_mu, resume_cond) = &*resume_pair;
                                let resume_state = resume_mu.lock().unwrap();
                                let resume_gen = resume_state.1;

                                drop(
                                    resume_cond
                                        .wait_while(resume_state, |(resume, gen)| {
                                            !*resume && *gen == resume_gen
                                        })
                                        .unwrap(),
                                );

                                let function_pool = env.function_pool(&mut u, &ctx);
                                let function = *u
                                    .choose(&function_pool)
                                    .wrap_err("failed to choose function from pool")?;

                                tracing::trace!(function = function.name, "New call");

                                let (pause_mu, pause_cond) = &*pause_pair;
                                let mut pause_state = pause_mu.lock().unwrap();
                                let pause_gen = pause_state.1;

                                pause_state.0 += 1;

                                if pause_state.0 == n_live_threads.load(atomic::Ordering::SeqCst) {
                                    resume_mu.lock().unwrap().0 = false;
                                    pause_state.0 = 0;
                                    pause_state.1 = pause_state.1.wrapping_add(1);
                                    pause_cond.notify_all();
                                    tx.send(()).unwrap();
                                }

                                drop(
                                    pause_cond
                                        .wait_while(pause_state, |(n, gen)| {
                                            *n != n_live_threads.load(atomic::Ordering::SeqCst)
                                                && pause_gen == *gen
                                        })
                                        .unwrap(),
                                );
                            }

                            Ok(())
                        }
                    })
                    .wrap_err("failed to spawn thread")?;
            }

            thread::Builder::new()
                .name("wazzi-differ".to_owned())
                .spawn_scoped(scope, {
                    move || {
                        while let Ok(_) = rx.recv() {
                            let (resume_mu, resume_cond) = &*resume_pair;
                            let mut resume_state = resume_mu.lock().unwrap();

                            resume_state.0 = true;
                            resume_state.1 = resume_state.1.wrapping_add(1);
                            resume_cond.notify_all();
                        }
                    }
                })
                .wrap_err("failed to spawn differ thread")?;

            Ok(())
        })
        .wrap_err("failed to fuzz")?;

        Ok(())
    }
}
