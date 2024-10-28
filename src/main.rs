#![feature(trait_upcasting)]

use std::{
    fs,
    io,
    ops::Deref,
    panic,
    path::{Path, PathBuf},
    process,
    sync::{
        atomic::{self, AtomicBool, AtomicUsize},
        mpsc,
        Arc,
        Condvar,
        Mutex,
        RwLock,
    },
    thread,
    time::Duration,
};

use arbitrary::Unstructured;
use clap::Parser;
use eyre::{eyre as err, Context as _};
use itertools::{EitherOrBoth, Itertools as _};
use rand::{thread_rng, RngCore};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
use walkdir::WalkDir;
use wazzi::{
    apply_env_initializers,
    normalization::InitializeState,
    spec::Spec,
    Call,
    CallStrategy,
    EnvironmentInitializer,
    RuntimeContext,
    StatefulStrategy,
    StatelessStrategy,
};
use wazzi_runners::{MappedDir, Node, RunningExecutor, Wamr, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_store::FuzzStore;

#[derive(Parser, Debug)]
struct Cmd {
    #[arg(long)]
    data: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = Strategy::Stateful)]
    strategy: Strategy,

    #[arg(long)]
    max_epochs: Option<usize>,

    #[arg(long, value_parser = humantime::parse_duration)]
    duration: Option<Duration>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Strategy {
    Stateful,
    Stateless,
}

impl Strategy {
    fn into_call_strategy<'a>(
        self,
        u: &'a mut Unstructured,
        ctx: &'a RuntimeContext,
        z3_ctx: &'a z3::Context,
    ) -> Box<dyn CallStrategy + 'a> {
        match self {
            | Strategy::Stateful => Box::new(StatefulStrategy::new(u, ctx, z3_ctx)),
            | Strategy::Stateless => Box::new(StatelessStrategy::new(u, ctx)),
        }
    }
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::Registry::default()
            .with(
                EnvFilter::builder()
                    .with_env_var("WAZZI_LOG_LEVEL")
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(ErrorLayer::default())
            .with(
                tracing_subscriber::fmt::layer()
                    .with_thread_names(true)
                    .with_writer(io::stderr)
                    .pretty(),
            ),
    )
    .wrap_err("failed to configure tracing")?;

    let orig_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    let cmd = Cmd::parse();
    let mut store = FuzzStore::new(Path::new("abc")).wrap_err("failed to init fuzz store")?;
    let mut fuzzer = Fuzzer::new(
        cmd.strategy,
        &mut store,
        [
            (
                "node",
                Box::new(Node::default()) as Box<dyn InitializeState>,
            ),
            (
                "wamr",
                Box::new(Wamr::default()) as Box<dyn InitializeState>,
            ),
            (
                "wasmedge",
                Box::new(Wasmedge::default()) as Box<dyn InitializeState>,
            ),
            (
                "wasmer",
                Box::new(Wasmer::default()) as Box<dyn InitializeState>,
            ),
            (
                "wasmtime",
                Box::new(Wasmtime::default()) as Box<dyn InitializeState>,
            ),
            (
                "wazero",
                Box::new(Wazero::default()) as Box<dyn InitializeState>,
            ),
        ],
        cmd.duration,
    );
    let data = cmd
        .data
        .as_ref()
        .map(|path| fs::read(path))
        .transpose()
        .wrap_err("failed to read data")?;

    fuzzer.fuzz_loop(data, cmd.max_epochs)?;

    Ok(())
}

#[derive(Debug)]
struct Fuzzer<'s> {
    strategy: Strategy,
    store:    &'s mut FuzzStore,
    runtimes: Vec<(&'static str, Box<dyn InitializeState>)>,
    duration: Option<Duration>,
}

impl<'s> Fuzzer<'s> {
    pub fn new(
        strategy: Strategy,
        store: &'s mut FuzzStore,
        runtimes: impl IntoIterator<Item = (&'static str, Box<dyn InitializeState>)>,
        duration: Option<Duration>,
    ) -> Self {
        Self {
            strategy,
            store,
            runtimes: runtimes.into_iter().collect(),
            duration,
        }
    }

    pub fn fuzz_loop(
        &mut self,
        data: Option<Vec<u8>>,
        max_epochs: Option<usize>,
    ) -> Result<(), eyre::Error> {
        let mut data = data;
        let mut epoch = 0;
        let cancel = Arc::new(AtomicBool::new(false));

        if let Some(duration) = self.duration.clone() {
            thread::Builder::new()
                .name("timer".to_owned())
                .spawn({
                    let cancel = cancel.clone();

                    move || {
                        thread::sleep(duration);
                        cancel.store(true, atomic::Ordering::SeqCst);
                        tracing::warn!(
                            duration = humantime::Duration::from(duration).to_string(),
                            "Time's up. Cancelling."
                        );
                    }
                })
                .wrap_err("failed to spawn timer thread")?;
        }

        loop {
            if cancel.load(atomic::Ordering::SeqCst) {
                tracing::info!("Fuzz loop cancelled.");
                break;
            }

            if let Some(max_epochs) = max_epochs {
                if epoch == max_epochs {
                    break;
                }
            }

            epoch += 1;

            match self.fuzz(epoch, data.take(), cancel.clone()) {
                | Ok(_) => continue,
                | Err(FuzzError::DiffFound) => continue,
                | Err(err) => return Err(err!(err)),
            }
        }

        Ok(())
    }

    pub fn fuzz(
        &mut self,
        epoch: usize,
        data: Option<Vec<u8>>,
        cancel_loop: Arc<AtomicBool>,
    ) -> Result<(), FuzzError> {
        let mut run_store = self
            .store
            .new_run()
            .wrap_err("failed to init new run store")?;
        let data = match data {
            | Some(d) => d,
            | None => {
                let mut data = vec![0u8; 131072];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };
        let spec = Spec::preview1().wrap_err("failed to init spec")?;
        let mut initializers: Vec<EnvironmentInitializer> = Default::default();
        let mut runtimes: Vec<_> = Default::default();

        for (runtime_name, runtime) in &self.runtimes {
            let store = run_store
                .new_runtime(runtime_name.to_string(), "-")
                .wrap_err("failed to init runtime store")?;
            let stderr = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(store.path.join("stderr"))
                .wrap_err("failed to open stderr file")?;
            let executor = RunningExecutor::from_wasi_runner(
                runtime.as_ref(),
                PathBuf::from("target/debug/wazzi-executor.wasm")
                    .canonicalize()
                    .unwrap()
                    .as_ref(),
                &store.path,
                Arc::new(Mutex::new(stderr)),
                vec![MappedDir {
                    name:      "base".to_string(),
                    host_path: store.base.clone(),
                }],
            )
            .unwrap();
            let initializer = runtime.initialize_state(
                &spec,
                &executor,
                vec![MappedDir {
                    name:      "base".to_string(),
                    host_path: store.base.clone(),
                }],
            )?;

            initializers.push(initializer);
            runtimes.push((runtime_name.to_string(), store, executor));
        }

        let (env, ctxs) = apply_env_initializers(&spec, &initializers);
        let env = Arc::new(RwLock::new(env));

        run_store
            .write_data(&data)
            .wrap_err("failed to write data")?;

        thread::scope(|scope| -> Result<_, FuzzError> {
            let (tx, rx) = mpsc::channel();
            let pause_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let resume_pair = Arc::new((Mutex::new((true, 0usize)), Condvar::new()));
            let n_live_threads = Arc::new(AtomicUsize::new(self.runtimes.len()));
            let mut runtime_threads = Vec::with_capacity(self.runtimes.len());
            let cancel = Arc::new(AtomicBool::new(false));

            for ((runtime_name, mut store, executor), mut ctx) in runtimes.into_iter().zip(ctxs) {
                runtime_threads.push(
                    thread::Builder::new()
                        .name(runtime_name.to_string())
                        .spawn_scoped(scope, {
                            let mut u = Unstructured::new(&data);
                            let env = env.clone();
                            let tx = tx.clone();
                            let cancel = cancel.clone();
                            let cancel_loop = cancel_loop.clone();
                            let pause_pair = pause_pair.clone();
                            let resume_pair = resume_pair.clone();
                            let n_live_threads = n_live_threads.clone();
                            let strategy = self.strategy;

                            move || -> Result<(), FuzzError> {
                                let z3_cfg = z3::Config::new();
                                let z3_ctx = z3::Context::new(&z3_cfg);
                                let spec = Spec::preview1()?;
                                let mut iteration = 0;

                                loop {
                                    let env = env.clone();
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

                                    if cancel.load(atomic::Ordering::SeqCst) {
                                        return Err(FuzzError::DiffFound);
                                    }

                                    if cancel_loop.load(atomic::Ordering::SeqCst) {
                                        return Err(FuzzError::Unknown(err!(
                                            "fuzz epoch cancelled"
                                        )));
                                    }

                                    if u.is_empty() {
                                        panic!("data exhausted");
                                    }

                                    let mut strategy =
                                        strategy.into_call_strategy(&mut u, &ctx, &z3_ctx);
                                    let function =
                                        strategy.select_function(&spec, &env.read().unwrap())?;

                                    tracing::info!(
                                        epoch = epoch,
                                        iteration = iteration,
                                        function = function.name,
                                        "Calling function."
                                    );
                                    iteration += 1;

                                    let (params, results) = match env.read().unwrap().call(
                                        &spec,
                                        store.trace_mut(),
                                        function,
                                        strategy.as_mut(),
                                        &executor,
                                    ) {
                                        | Ok(x) => x,
                                        | Err(err) => {
                                            let (pause_mu, _pause_cond) = &*pause_pair;
                                            let mut pause_state = pause_mu.lock().unwrap();

                                            pause_state.0 += 1;
                                            drop(pause_state);
                                            cancel.store(true, atomic::Ordering::SeqCst);

                                            return Err(FuzzError::Unknown(err));
                                        },
                                    };

                                    drop(strategy);

                                    let mut env_prev_iter = env.read().unwrap().deref().clone();

                                    if let Some(results) = &results {
                                        for (result_value, result) in
                                            results.into_iter().zip(function.results.iter())
                                        {
                                            env_prev_iter.add_resources_to_ctx_recursively(
                                                &spec,
                                                &mut ctx,
                                                result.tref.resolve(&spec),
                                                &result_value,
                                            );
                                        }
                                    }

                                    drop(env);

                                    let function = function.clone();
                                    let (pause_mu, pause_cond) = &*pause_pair;
                                    let mut pause_state = pause_mu.lock().unwrap();
                                    let pause_gen = pause_state.1;

                                    pause_state.0 += 1;

                                    if pause_state.0
                                        == n_live_threads.load(atomic::Ordering::SeqCst)
                                    {
                                        resume_mu.lock().unwrap().0 = false;
                                        pause_state.0 = 0;
                                        pause_state.1 = pause_state.1.wrapping_add(1);
                                        pause_cond.notify_all();
                                        tx.send((function, params, results, ctx.clone())).unwrap();
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
                            }
                        })
                        .wrap_err(format!("failed to spawn {runtime_name}"))?,
                );
            }

            thread::Builder::new()
                .name("wazzi-differ".to_string())
                .spawn_scoped(scope, {
                    let cancel = cancel.clone();
                    let mut u = Unstructured::new(&data);

                    move || -> Result<(), FuzzError> {
                        let spec = Spec::preview1()?;

                        while let Ok((function, params, results, ctx)) = rx.recv() {
                            let runtimes = run_store
                                .runtimes::<Call>()
                                .wrap_err("failed to resume runtimes")?
                                .collect::<Vec<_>>();

                            'outer: for (i, runtime_0) in runtimes.iter().enumerate() {
                                let call_0 = runtime_0
                                    .trace()
                                    .last_call()
                                    .wrap_err("failed to get last call")
                                    .unwrap()
                                    .unwrap()
                                    .read_call()
                                    .wrap_err("failed to read action")
                                    .unwrap()
                                    .unwrap();

                                for j in (i + 1)..runtimes.len() {
                                    let runtime_1 = runtimes.get(j).unwrap();
                                    let call_1 = runtime_1
                                        .trace()
                                        .last_call()
                                        .wrap_err("failed to get last call")
                                        .unwrap()
                                        .unwrap()
                                        .read_call()
                                        .wrap_err("failed to read action")
                                        .unwrap()
                                        .unwrap();

                                    match (call_0.errno, call_1.errno) {
                                        | (None, None) => {},
                                        | (Some(errno_0), Some(errno_1))
                                            if errno_0 == 0 && errno_1 == 0
                                                || errno_0 != 0 && errno_1 != 0 => {},
                                        | _ => {
                                            tracing::error!(
                                                runtime_a = runtime_0.name(),
                                                runtime_b = runtime_1.name(),
                                                runtime_a_errno = call_0.errno,
                                                runtime_b_errno = call_1.errno,
                                                "Errno diff found!"
                                            );

                                            cancel.store(true, atomic::Ordering::SeqCst);
                                            break 'outer;
                                        },
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
                                                        && fs::read(a.path())
                                                            .wrap_err("failed to read file")?
                                                            != fs::read(b.path())
                                                                .wrap_err("failed to read file")?)
                                                {
                                                    tracing::error!("Fs diff found.");

                                                    cancel.store(true, atomic::Ordering::SeqCst);
                                                    break 'outer;
                                                }
                                            },
                                            | EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                                                tracing::error!("Fs diff found.");

                                                cancel.store(true, atomic::Ordering::SeqCst);
                                                break 'outer;
                                            },
                                        }
                                    }
                                }
                            }

                            if !cancel.load(atomic::Ordering::SeqCst) {
                                if let Some(results) = results {
                                    let z3_cfg = z3::Config::new();
                                    let z3_ctx = z3::Context::new(&z3_cfg);
                                    let results = function
                                        .results
                                        .iter()
                                        .zip(results)
                                        .map(|(result, result_value)| {
                                            (result.name.clone(), result_value)
                                        })
                                        .collect_vec();
                                    let mut env = env.write().unwrap();
                                    let result_resource_idxs = env.execute_function_effects(
                                        &spec, &function, &params, &results,
                                    );
                                    let mut strategy =
                                        self.strategy.into_call_strategy(&mut u, &ctx, &z3_ctx);

                                    strategy
                                        .handle_results(
                                            &spec,
                                            &function,
                                            &mut env,
                                            params,
                                            result_resource_idxs,
                                        )
                                        .unwrap();
                                }
                            }

                            let (resume_mu, resume_cond) = &*resume_pair;
                            let mut resume_state = resume_mu.lock().unwrap();

                            resume_state.0 = true;
                            resume_state.1 = resume_state.1.wrapping_add(1);
                            resume_cond.notify_all();
                        }

                        Ok(())
                    }
                })
                .wrap_err("failed to spawn differ thread")?;

            for runtime_thread in runtime_threads {
                runtime_thread.join().unwrap()?;
            }

            Ok(())
        })?;

        Err(FuzzError::Unknown(err!("fuzz loop ended")))
    }
}

#[derive(thiserror::Error, Debug)]
enum FuzzError {
    #[error(transparent)]
    Unknown(#[from] eyre::Error),

    #[error("execution diff found")]
    DiffFound,
}
