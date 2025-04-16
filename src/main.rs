use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{self, stderr, IsTerminal},
    panic,
    path::{Path, PathBuf},
    process,
    sync::{
        atomic::{self, AtomicBool},
        Arc,
        Condvar,
        Mutex,
        RwLock,
    },
    thread,
    time::Duration,
};

use arbitrary::Unstructured;
use clap::{builder::TypedValueParser, Parser};
use eyre::{eyre as err, Context as _};
use itertools::{EitherOrBoth, Itertools as _};
use memmap::MmapOptions;
use multiqueue::broadcast_queue;
use rand::{thread_rng, RngCore};
use serde::{Deserialize, Serialize};
use threadpool::ThreadPool;
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
use walkdir::WalkDir;
use wazzi::{
    apply_env_initializers,
    execute_call,
    normalization::Runtime,
    spec::{Spec, WasiValue},
    Call,
    CallStrategy,
    EnvironmentInitializer,
    MaybeResourceValue,
    ResourceIdx,
    StatefulStrategy,
    StatelessStrategy,
};
use wazzi_runners::{MappedDir, Node, RunningExecutor, Wamr, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_store::Store;

static BUF_SIZE: usize = 131072;

#[derive(Clone, Debug)]
struct HumantimeParser;

impl TypedValueParser for HumantimeParser {
    type Value = Duration;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        Ok(humantime::parse_duration(&value.to_string_lossy())
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ValueValidation, e))?)
    }
}

#[derive(Parser, Debug)]
struct Cmd {
    #[arg(long)]
    data: Option<PathBuf>,

    #[arg()]
    config: PathBuf,

    #[arg()]
    path: PathBuf,

    #[arg(long, value_enum, default_value_t = Strategy::Stateful)]
    strategy: Strategy,

    #[arg(long, value_parser = HumantimeParser)]
    time_limit: Option<Duration>,

    #[arg(short = 'c', default_value = "1")]
    fuzzer_count: usize,

    #[arg(long, default_value_t = false)]
    silent: bool,
}

#[derive(clap::ValueEnum, Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
enum Strategy {
    Stateful,
    Stateless,
}

impl Strategy {
    fn into_call_strategy<'a>(
        self,
        u: &'a mut Unstructured,
        ctx: &'a z3::Context,
        preopens: BTreeMap<ResourceIdx, PathBuf>,
    ) -> Box<dyn CallStrategy + 'a> {
        match self {
            | Strategy::Stateful => Box::new(StatefulStrategy::new(u, ctx, preopens)),
            | Strategy::Stateless => Box::new(StatelessStrategy::new(u)),
        }
    }
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let cmd = Cmd::parse();

    if !cmd.silent {
        let mut subscriber = tracing_subscriber::fmt::layer()
            .with_thread_names(true)
            .with_writer(io::stderr);

        if !stderr().is_terminal() {
            subscriber.set_ansi(false);
        }

        tracing::subscriber::set_global_default(
            tracing_subscriber::Registry::default()
                .with(
                    EnvFilter::builder()
                        .with_env_var("DIVA_LOG_LEVEL")
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env_lossy(),
                )
                .with(ErrorLayer::default())
                .with(subscriber),
        )
        .wrap_err("failed to configure tracing")?;
    }

    let orig_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    let config: FuzzConfig = serde_yml::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(&cmd.config)
            .wrap_err("failed to read fuzz config")?,
    )
    .wrap_err("failed to deserialize fuzz config")?;

    fs::create_dir(&cmd.path)?;

    let store = Store::new(&cmd.path).wrap_err("failed to init fuzz store")?;
    let mut runtimes = Vec::with_capacity(config.runtimes.len());

    for runtime in config.runtimes {
        let rt = match runtime.name.as_str() {
            | "node" => Box::new(Node::default()) as Box<dyn Runtime>,
            | "wamr" => Box::new(Wamr::default()) as Box<dyn Runtime>,
            | "wasmedge" => Box::new(Wasmedge::default()) as Box<dyn Runtime>,
            | "wasmer" => Box::new(Wasmer::default()) as Box<dyn Runtime>,
            | "wasmtime" => Box::new(Wasmtime::default()) as Box<dyn Runtime>,
            | "wazero" => Box::new(Wazero::default()) as Box<dyn Runtime>,
            | name @ _ => return Err(err!("unknown runtime {name}")),
        };

        runtimes.push((runtime.name, rt));
    }

    let mut fuzzer = Fuzzer::new(
        fs::read_to_string(config.spec).wrap_err("failed to read spec file")?,
        cmd.strategy,
        store,
        runtimes,
        cmd.silent,
    );

    if let Some(data) = cmd.data {
        fuzzer.fuzz(data)?;
    } else {
        fuzzer.fuzz_loop(cmd.fuzzer_count, cmd.time_limit)?;
    }

    Ok(())
}

#[derive(Debug)]
struct Fuzzer {
    silent:   bool,
    spec:     String,
    strategy: Strategy,
    store:    Arc<Store>,
    runtimes: Vec<(String, Box<dyn Runtime>)>,
}

impl Fuzzer {
    pub fn new(
        spec: String,
        strategy: Strategy,
        store: Store,
        runtimes: impl IntoIterator<Item = (String, Box<dyn Runtime>)>,
        silent: bool,
    ) -> Self {
        Self {
            silent,
            spec,
            strategy,
            store: Arc::new(store),
            runtimes: runtimes.into_iter().collect(),
        }
    }

    pub fn fuzz(&mut self, data: PathBuf) -> Result<(), eyre::Error> {
        let log_trace = !self.silent;
        let data = fs::read(data)?;
        let store = self.store.clone();
        let spec = self.spec.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let strategy = self.strategy.clone();
        let runtime_initializers = Arc::new(self.runtimes.clone());
        let over = Arc::new(AtomicBool::new(false));

        thread::scope(|scope| -> Result<(), eyre::Error> {
            let (run_id, mut run) = store.new_run::<Call>()?;
            let spec = Spec::preview1(&spec).wrap_err("failed to init spec")?;
            let mut initializers: Vec<(String, EnvironmentInitializer)> = Default::default();
            let mut runtimes: Vec<_> = Default::default();

            for (runtime_name, runtime) in runtime_initializers.iter() {
                let store = run
                    .new_runtime(runtime_name.to_string(), log_trace)
                    .wrap_err("failed to init runtime store")?;
                let executor = {
                    let store = store.read().unwrap();
                    let stderr = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(store.root_path().join("stderr"))
                        .wrap_err("failed to open stderr file")?;
                    let executor = RunningExecutor::from_wasi_runner(
                        runtime.as_ref(),
                        Path::new("target").join("release").join("wazzi-executor.wasm").as_ref(),
                        store.root_path(),
                        Arc::new(Mutex::new(stderr)),
                        vec![MappedDir {
                            name:      "base".to_string(),
                            host_path: store.base_path().to_path_buf(),
                        }],
                    )
                    .unwrap();
                    let initializer = runtime.initialize_state(
                        runtime_name.clone(),
                        &spec,
                        &executor,
                        vec![MappedDir {
                            name:      "base".to_string(),
                            host_path: store.base_path().to_path_buf(),
                        }],
                    )?;

                    initializers.push((runtime_name.to_string(), initializer));

                    executor
                };

                runtimes.push((runtime_name.to_string(), store, executor));
            }

            let run = Arc::new(Mutex::new(run));
            let n_runtimes = runtimes.len();
            let rts = initializers.iter().map(|(name, _)| name.to_string()).collect_vec();
            let (env, rtctxs, preopens) =
                apply_env_initializers(&spec, &initializers.into_iter().map(|p| p.1).collect_vec());
            let env = Arc::new(RwLock::new(env));
            let rtctxs = Arc::new(RwLock::new(rtctxs));
            let fill_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (fill_done_tx, fill_done_rx) = broadcast_queue(1);
            let mmap = Arc::new(Mutex::new(MmapOptions::new().len(BUF_SIZE).map_anon().unwrap()));

            thread::Builder::new()
                .name(format!("filler-{run_id}"))
                .spawn_scoped(scope, {
                    let run = run.clone();
                    let fill_init = fill_init.clone();
                    let mmap = mmap.clone();
                    let over = over.clone();
                    let cancel = cancel.clone();

                    move || {
                        run.lock().unwrap().configure_progress_logging(!self.silent);

                        'outer: loop {
                            loop {
                                let (mu, cond) = &*fill_init;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(state, Duration::from_micros(100), |(ready, g)| {
                                        *ready != n_runtimes && gen == *g
                                    })
                                    .unwrap();

                                if over.load(atomic::Ordering::SeqCst) {
                                    break 'outer;
                                }

                                if cancel.load(atomic::Ordering::SeqCst) {
                                    over.store(true, atomic::Ordering::SeqCst);
                                    break 'outer;
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);

                                break;
                            }

                            thread_rng().fill_bytes(&mut mmap.lock().unwrap());
                            run.lock().unwrap().write_data(&mmap.lock().unwrap()).unwrap();
                            fill_done_tx.try_send(()).unwrap();
                        }

                        tracing::info!("Fuzz over. Stopping thread.");
                    }
                })
                .wrap_err("failed to spawn buf filler thread")?;

            let select_func_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (select_func_done_tx, select_func_done_rx) = broadcast_queue(1);
            let prep_params_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (prep_params_done_tx, prep_params_done_rx) = broadcast_queue(1);
            let lift_results_init = Arc::new((
                Mutex::new((0, 0usize, HashMap::<String, Option<Vec<WasiValue>>>::new(), None::<i32>)),
                Condvar::new(),
            ));
            let (lift_results_done_tx, lift_results_done_rx) = broadcast_queue(1);
            let solve_output_contract_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (solve_output_contract_done_tx, solve_output_contract_done_rx) = broadcast_queue(1);
            let diff_init = Arc::new((Mutex::new((0, 0usize, None)), Condvar::new()));
            let (diff_done_tx, diff_done_rx) = broadcast_queue(1);

            thread::Builder::new()
                .name(format!("strat-{run_id}"))
                .spawn_scoped(scope, {
                    let data = data.clone();
                    let env = env.clone();
                    let run = run.clone();
                    let spec = spec.clone();
                    let over = over.clone();
                    let select_func_init = select_func_init.clone();
                    let prep_params_init = prep_params_init.clone();
                    let lift_results_init_pair = lift_results_init.clone();
                    let solve_output_contract_init = solve_output_contract_init.clone();
                    let rtctxs = rtctxs.clone();

                    move || {
                        run.lock().unwrap().configure_progress_logging(log_trace);

                        let mut u = Unstructured::new(&data);
                        let cfg = z3::Config::new();
                        let ctx = z3::Context::new(&cfg);
                        let mut strategy = strategy.into_call_strategy(&mut u, &ctx, preopens);

                        'outer: loop {
                            loop {
                                let (mu, cond) = &*select_func_init;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(state, Duration::from_micros(100), |(select, g)| {
                                        *select != n_runtimes && *g == gen
                                    })
                                    .unwrap();

                                if over.load(atomic::Ordering::SeqCst) {
                                    break 'outer;
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                                break;
                            }

                            let function = strategy.select_function(&spec, &env.read().unwrap()).unwrap();

                            select_func_done_tx.try_send(function.to_owned()).unwrap();

                            loop {
                                let (mu, cond) = &*prep_params_init;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(state, Duration::from_micros(100), |(ready, g)| {
                                        (*ready != n_runtimes) && *g == gen
                                    })
                                    .unwrap();

                                if over.load(atomic::Ordering::SeqCst) {
                                    break 'outer;
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);

                                break;
                            }

                            let params = strategy
                                .prepare_arguments(&spec, function, &env.read().unwrap())
                                .unwrap();

                            prep_params_done_tx.try_send(params.clone()).unwrap();

                            let (results, errno) = loop {
                                let (mu, cond) = &*lift_results_init_pair;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(
                                        state,
                                        Duration::from_micros(100),
                                        |(ready, g, _results, _errno)| (*ready != n_runtimes) && *g == gen,
                                    )
                                    .unwrap();
                                let mut results = HashMap::new();

                                if over.load(atomic::Ordering::SeqCst) {
                                    break 'outer;
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                                std::mem::swap(&mut state.2, &mut results);

                                break (results, state.3.take());
                            };

                            let mut resource_idxs = Vec::new();

                            if errno.is_none() || errno.unwrap() == 0 {
                                for (i, _result) in function.results.iter().enumerate() {
                                    let mut result_values = Vec::new();
                                    let tdef = function.results.get(i).unwrap().tref.resolve(&spec);

                                    for rt in rts.iter() {
                                        let results = results.get(rt).unwrap();

                                        if let Some(results) = results {
                                            result_values.push(results.get(i).unwrap());
                                        }
                                    }

                                    let mut rtctxs = rtctxs.write().unwrap();
                                    let ctxs = rtctxs.iter_mut().zip(result_values).collect_vec();
                                    let resource_idx = env.write().unwrap().lift_recursively(&spec, ctxs, tdef);

                                    resource_idxs.push(resource_idx);
                                }
                            }

                            lift_results_done_tx.try_send(resource_idxs.clone()).unwrap();

                            loop {
                                let (mu, cond) = &*solve_output_contract_init;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(state, Duration::from_millis(100), |(ready, g)| {
                                        (*ready != n_runtimes) && *g == gen
                                    })
                                    .unwrap();

                                if over.load(atomic::Ordering::SeqCst) {
                                    tracing::info!("Fuzz over. Stopping thread.");
                                    break 'outer;
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);

                                break;
                            }

                            if errno.is_none() || errno.unwrap() == 0 {
                                let result_values = results.get(rts.first().unwrap()).unwrap().clone();

                                strategy
                                    .handle_results(
                                        &spec,
                                        function,
                                        &mut env.write().unwrap(),
                                        params,
                                        resource_idxs,
                                        result_values.as_ref().map(Vec::as_slice),
                                    )
                                    .unwrap();
                            }

                            solve_output_contract_done_tx.try_send(()).unwrap();
                        }

                        tracing::info!("Strategy thread exiting.");
                    }
                })?;

            thread::Builder::new()
                .name(format!("diff-{run_id}"))
                .spawn_scoped(scope, {
                    let run = run.clone();
                    let over = over.clone();
                    let cancel = cancel.clone();
                    let diff_init = diff_init.clone();

                    move || -> Result<(), FuzzError> {
                        run.lock().unwrap().configure_progress_logging(log_trace);

                        loop {
                            let errnos: Vec<_> = loop {
                                let (mu, cond) = &*diff_init;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let (mut state, result) = cond
                                    .wait_timeout_while(state, Duration::from_micros(100), |(ready, g, _)| {
                                        *ready != n_runtimes && gen == *g
                                    })
                                    .unwrap();

                                if over.load(atomic::Ordering::SeqCst) || cancel.load(atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }

                                if result.timed_out() {
                                    continue;
                                }

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);

                                break state.2.take().unwrap();
                            };

                            let first = errnos.first().unwrap();

                            for (_runtime_name, errno) in errnos.iter().skip(1) {
                                match (first.1, errno) {
                                    | (None, None) => continue,
                                    | (None, Some(_)) | (Some(_), None) => {
                                        tracing::info!("Errno diff found.");
                                        diff_done_tx.try_send(DiffResult::Errno).unwrap();
                                        return Ok(());
                                    },
                                    | (Some(l), &Some(r)) => {
                                        if (l == 0 && r != 0) || (l != 0 && r == 0) {
                                            tracing::info!("Errno diff found.");
                                            diff_done_tx.try_send(DiffResult::Errno).unwrap();
                                            return Ok(());
                                        }
                                    },
                                }
                            }

                            let run = run.lock().unwrap();
                            let runtimes = run.runtime_stores().collect::<Vec<_>>();

                            'outer: for (i, (runtime_0_name, runtime_0)) in runtimes.iter().enumerate() {
                                let runtime_0 = runtime_0.read().unwrap();
                                let call_0 = runtime_0.last_call().unwrap();

                                for j in (i + 1)..runtimes.len() {
                                    let (runtime_1_name, runtime_1) = runtimes.get(j).unwrap();
                                    let runtime_1 = runtime_1.read().unwrap();
                                    let call_1 = runtime_1.last_call().unwrap();

                                    match (call_0.errno, call_1.errno) {
                                        | (None, None) => {},
                                        | (Some(errno_0), Some(errno_1))
                                            if errno_0 == 0 && errno_1 == 0 || errno_0 != 0 && errno_1 != 0 => {},
                                        | _ => {
                                            tracing::error!(
                                                runtime_a = runtime_0_name,
                                                runtime_b = runtime_1_name,
                                                runtime_a_errno = call_0.errno,
                                                runtime_b_errno = call_1.errno,
                                                "Errno diff found!"
                                            );

                                            over.store(true, atomic::Ordering::SeqCst);
                                            break 'outer;
                                        },
                                    }

                                    let runtime_0_walk = WalkDir::new(&runtime_0.base_path())
                                        .sort_by_file_name()
                                        .min_depth(1)
                                        .into_iter();
                                    let runtime_1_walk = WalkDir::new(&runtime_1.base_path())
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
                                                        && fs::read(a.path()).wrap_err("failed to read file")?
                                                            != fs::read(b.path()).wrap_err("failed to read file")?)
                                                {
                                                    tracing::error!("Fs diff found.");
                                                    diff_done_tx.try_send(DiffResult::Filesystem).unwrap();
                                                    return Ok(());
                                                }
                                            },
                                            | EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                                                tracing::error!("Fs diff found.");
                                                diff_done_tx.try_send(DiffResult::Filesystem).unwrap();
                                                return Ok(());
                                            },
                                        }
                                    }
                                }
                            }

                            diff_done_tx.try_send(DiffResult::Ok).unwrap();
                        }
                    }
                })
                .wrap_err("failed to spawn differ thread")?;

            let mut runtime_threads = Vec::new();

            for (i, (runtime_name, store, executor)) in runtimes.into_iter().enumerate() {
                runtime_threads.push(
                    thread::Builder::new()
                        .name(format!("drv-{run_id}-{runtime_name}"))
                        .spawn_scoped(scope, {
                            let run_id = run_id.clone();
                            let data = data.clone();
                            let run = run.clone();
                            let over = over.clone();
                            let cancel = cancel.clone();
                            let diff_init = diff_init.clone();
                            let diff_done_rx = diff_done_rx.add_stream();
                            let select_func_init = select_func_init.clone();
                            let select_func_done_rx = select_func_done_rx.add_stream();
                            let prep_params_init = prep_params_init.clone();
                            let prep_params_done_rx = prep_params_done_rx.add_stream();
                            let lift_results_init = lift_results_init.clone();
                            let lift_results_done_rx = lift_results_done_rx.add_stream();
                            let solve_output_contract_init = solve_output_contract_init.clone();
                            let solve_output_contract_done_rx = solve_output_contract_done_rx.add_stream();
                            let spec = spec.clone();
                            let rtctxs = rtctxs.clone();
                            let runtime_name = runtime_name.clone();

                            move || -> Result<(), FuzzError> {
                                run.lock().unwrap().configure_progress_logging(log_trace);

                                let mut iteration = 0;
                                let u = Unstructured::new(&data);

                                loop {
                                    if cancel.load(atomic::Ordering::SeqCst) {
                                        tracing::info!("Cancelling fuzz run.");
                                        over.store(true, atomic::Ordering::SeqCst);
                                        return Err(FuzzError::Time);
                                    }

                                    {
                                        // Let the strategy thread select a function.

                                        let (mu, cond) = &*select_func_init;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    let function = select_func_done_rx.recv().unwrap();

                                    tracing::info!(
                                        run_id = run_id,
                                        iteration = iteration,
                                        function = function.name,
                                        "Calling function."
                                    );
                                    iteration += 1;

                                    {
                                        let (mu, cond) = &*prep_params_init;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            // Dispatch to strategy thread to select a function.
                                            cond.notify_all();
                                        }
                                    }

                                    let params = match prep_params_done_rx.recv() {
                                        | Ok(x) => x,
                                        | Err(_) => {
                                            tracing::info!("Strategy thread terminated. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                    };
                                    let (errno, results) = execute_call(
                                        &spec,
                                        rtctxs.read().unwrap().get(i).unwrap(),
                                        &function,
                                        params.clone(),
                                        &executor,
                                    )
                                    .unwrap();

                                    {
                                        let (mu, cond) = &*lift_results_init;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;
                                        state.2.insert(runtime_name.clone(), results.clone());
                                        state.3 = errno;

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    let resource_idxs = match lift_results_done_rx.recv() {
                                        | Ok(x) => x,
                                        | Err(_) => {
                                            tracing::info!("Strategy thread terminated. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                    };

                                    store
                                        .write()
                                        .unwrap()
                                        .record_call(Call {
                                            function: function.name,
                                            errno:    errno,
                                            params:   params
                                                .iter()
                                                .map(|p| {
                                                    let (value, resource_idx) =
                                                        rtctxs.read().unwrap().get(i).unwrap().lower(p.clone());

                                                    MaybeResourceValue { value, resource_idx }
                                                })
                                                .collect_vec(),
                                            results:  results.map(|results| {
                                                results
                                                    .iter()
                                                    .zip(resource_idxs)
                                                    .map(|(value, resource_idx)| MaybeResourceValue {
                                                        value: value.to_owned(),
                                                        resource_idx,
                                                    })
                                                    .collect_vec()
                                            }),
                                        })
                                        .unwrap();

                                    if u.is_empty() {
                                        panic!();
                                    }

                                    {
                                        // Start diff when all runtimes ready.

                                        let (mu, cond) = &*diff_init;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        let errno = (runtime_name.clone(), errno);

                                        match &mut state.2 {
                                            | Some(results) => results.push(errno),
                                            | None => state.2 = Some(vec![errno]),
                                        }

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    let diff_result = match diff_done_rx.recv() {
                                        | Ok(result) => result,
                                        | Err(_) => {
                                            tracing::info!("Diff thread terminated. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                    };

                                    match diff_result {
                                        | DiffResult::Ok => (),
                                        | DiffResult::Errno => {
                                            tracing::info!("Errno diff found. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                        | DiffResult::Filesystem => {
                                            tracing::info!("Filesystem diff found. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                    }
                                    {
                                        let (mu, cond) = &*solve_output_contract_init;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    match solve_output_contract_done_rx.recv() {
                                        | Ok(_) => (),
                                        | Err(_) => {
                                            tracing::info!("Diff thread terminated. Stopping fuzz run.");
                                            over.store(true, atomic::Ordering::SeqCst);
                                            break;
                                        },
                                    }
                                }

                                Ok(())
                            }
                        })
                        .wrap_err(format!("failed to spawn {runtime_name}"))?,
                );
            }

            fill_done_rx.unsubscribe();
            select_func_done_rx.unsubscribe();
            prep_params_done_rx.unsubscribe();
            lift_results_done_rx.unsubscribe();
            solve_output_contract_done_rx.unsubscribe();
            diff_done_rx.unsubscribe();

            Ok(())
        })
    }

    pub fn fuzz_loop(&mut self, fuzzer_count: usize, time_limit: Option<Duration>) -> Result<(), eyre::Error> {
        let enable_logging = !self.silent;
        let cancel = Arc::new(AtomicBool::new(false));

        if let Some(limit) = &time_limit {
            thread::Builder::new()
                .name(format!("timer"))
                .spawn({
                    let cancel = cancel.clone();
                    let limit = limit.to_owned();

                    move || {
                        thread::sleep(limit);
                        cancel.store(true, atomic::Ordering::SeqCst);
                        tracing::warn!(
                            duration = humantime::Duration::from(limit).to_string(),
                            "Time's up. Cancelling."
                        );
                    }
                })
                .wrap_err("failed to spawn timer thread")?;
        }

        let pool = ThreadPool::new(fuzzer_count);
        let runtime_initializers = Arc::new(self.runtimes.clone());

        while !cancel.load(atomic::Ordering::SeqCst) {
            if pool.active_count() + pool.queued_count() >= pool.max_count() {
                thread::sleep(Duration::from_millis(100));
                continue;
            }

            pool.execute({
                let store = self.store.clone();
                let (run_id, run) = store.new_run::<Call>()?;
                let run = Arc::new(Mutex::new(run));
                let run_ = run.clone();
                let spec = self.spec.clone();
                let cancel = cancel.clone();
                let strategy = self.strategy.clone();
                let runtime_initializers = runtime_initializers.clone();
                let over = Arc::new(AtomicBool::new(false));

                move || {
                    thread::scope(|scope| -> Result<(), eyre::Error> {
                        let spec = Spec::preview1(&spec).wrap_err("failed to init spec")?;
                        let mut initializers: Vec<(String, EnvironmentInitializer)> = Default::default();
                        let mut runtimes: Vec<_> = Default::default();

                        for (runtime_name, runtime) in runtime_initializers.iter() {
                            let mut run = run.lock().unwrap();
                            let store = run
                                .new_runtime(runtime_name.to_string(), enable_logging)
                                .wrap_err("failed to init runtime store")?;
                            let executor = {
                                let store = store.read().unwrap();
                                let stderr = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(store.root_path().join("stderr"))
                                    .wrap_err("failed to open stderr file")?;
                                let executor = RunningExecutor::from_wasi_runner(
                                    runtime.as_ref(),
                                    Path::new("target").join("release").join("wazzi-executor.wasm").as_ref(),
                                    &store.root_path(),
                                    Arc::new(Mutex::new(stderr)),
                                    vec![MappedDir {
                                        name:      "base".to_string(),
                                        host_path: store.base_path().to_path_buf(),
                                    }],
                                )
                                .unwrap();
                                let initializer = runtime.initialize_state(
                                    runtime_name.clone(),
                                    &spec,
                                    &executor,
                                    vec![MappedDir {
                                        name:      "base".to_string(),
                                        host_path: store.base_path().to_path_buf(),
                                    }],
                                )?;

                                initializers.push((runtime_name.to_string(), initializer));

                                executor
                            };

                            runtimes.push((runtime_name.to_string(), store, executor));
                        }

                        let run = run.clone();
                        let n_runtimes = runtimes.len();
                        let rts = initializers.iter().map(|(name, _)| name.to_string()).collect_vec();
                        let (env, rtctxs, preopens) =
                            apply_env_initializers(&spec, &initializers.into_iter().map(|p| p.1).collect_vec());
                        let env = Arc::new(RwLock::new(env));
                        let rtctxs = Arc::new(RwLock::new(rtctxs));
                        let fill_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
                        let (fill_done_tx, fill_done_rx) = broadcast_queue(1);
                        let mmap = Arc::new(Mutex::new(MmapOptions::new().len(BUF_SIZE).map_anon().unwrap()));

                        thread::Builder::new()
                            .name(format!("filler-{run_id}"))
                            .spawn_scoped(scope, {
                                let run = run.clone();
                                let fill_init = fill_init.clone();
                                let mmap = mmap.clone();
                                let over = over.clone();
                                let cancel = cancel.clone();

                                move || {
                                    run.lock().unwrap().configure_progress_logging(enable_logging);

                                    'outer: loop {
                                        loop {
                                            let (mu, cond) = &*fill_init;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(state, Duration::from_micros(100), |(ready, g)| {
                                                    *ready != n_runtimes && gen == *g
                                                })
                                                .unwrap();

                                            if over.load(atomic::Ordering::SeqCst) {
                                                break 'outer;
                                            }

                                            if cancel.load(atomic::Ordering::SeqCst) {
                                                over.store(true, atomic::Ordering::SeqCst);
                                                break 'outer;
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);

                                            break;
                                        }

                                        thread_rng().fill_bytes(&mut mmap.lock().unwrap());
                                        run.lock().unwrap().write_data(&mmap.lock().unwrap()).unwrap();
                                        fill_done_tx.try_send(()).unwrap();
                                    }

                                    tracing::info!("Fuzz over. Stopping thread.");
                                }
                            })
                            .wrap_err("failed to spawn buf filler thread")?;

                        let mut data = {
                            {
                                let (mu, cond) = &*fill_init;
                                let mut state = mu.lock().unwrap();

                                state.0 = n_runtimes;
                                cond.notify_all();
                            }

                            match fill_done_rx.recv() {
                                | Ok(_) => (),
                                | Err(_) => return Ok(()),
                            }

                            unsafe { std::slice::from_raw_parts(mmap.lock().unwrap().as_ptr(), BUF_SIZE) }
                        };
                        let select_func_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
                        let (select_func_done_tx, select_func_done_rx) = broadcast_queue(1);
                        let prep_params_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
                        let (prep_params_done_tx, prep_params_done_rx) = broadcast_queue(1);
                        let lift_results_init = Arc::new((
                            Mutex::new((0, 0usize, HashMap::<String, Option<Vec<WasiValue>>>::new(), None::<i32>)),
                            Condvar::new(),
                        ));
                        let (lift_results_done_tx, lift_results_done_rx) = broadcast_queue(1);
                        let solve_output_contract_init = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
                        let (solve_output_contract_done_tx, solve_output_contract_done_rx) = broadcast_queue(1);
                        let diff_init = Arc::new((Mutex::new((0, 0usize, None)), Condvar::new()));
                        let (diff_done_tx, diff_done_rx) = broadcast_queue(1);

                        thread::Builder::new()
                            .name(format!("strat-{run_id}"))
                            .spawn_scoped(scope, {
                                let data = data.to_vec();
                                let env = env.clone();
                                let run = run.clone();
                                let spec = spec.clone();
                                let over = over.clone();
                                let select_func_init = select_func_init.clone();
                                let prep_params_init = prep_params_init.clone();
                                let lift_results_init_pair = lift_results_init.clone();
                                let solve_output_contract_init = solve_output_contract_init.clone();
                                let rtctxs = rtctxs.clone();

                                move || {
                                    run.lock().unwrap().configure_progress_logging(enable_logging);

                                    let mut u = Unstructured::new(&data);
                                    let cfg = z3::Config::new();
                                    let ctx = z3::Context::new(&cfg);
                                    let mut strategy = strategy.into_call_strategy(&mut u, &ctx, preopens);

                                    'outer: loop {
                                        loop {
                                            let (mu, cond) = &*select_func_init;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(state, Duration::from_micros(100), |(select, g)| {
                                                    *select != n_runtimes && *g == gen
                                                })
                                                .unwrap();

                                            if over.load(atomic::Ordering::SeqCst) {
                                                break 'outer;
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);
                                            break;
                                        }

                                        let function = strategy.select_function(&spec, &env.read().unwrap()).unwrap();

                                        match select_func_done_tx.try_send(function.to_owned()) {
                                            | Ok(_) => (),
                                            | Err(_err) => {
                                                over.store(true, atomic::Ordering::Release);
                                                break;
                                            },
                                        }

                                        loop {
                                            let (mu, cond) = &*prep_params_init;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(state, Duration::from_micros(100), |(ready, g)| {
                                                    (*ready != n_runtimes) && *g == gen
                                                })
                                                .unwrap();

                                            if over.load(atomic::Ordering::SeqCst) {
                                                break 'outer;
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);

                                            break;
                                        }

                                        let params = strategy
                                            .prepare_arguments(&spec, function, &env.read().unwrap())
                                            .unwrap();

                                        prep_params_done_tx.try_send(params.clone()).unwrap();

                                        let (results, errno) = loop {
                                            let (mu, cond) = &*lift_results_init_pair;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(
                                                    state,
                                                    Duration::from_micros(100),
                                                    |(ready, g, _results, _errno)| (*ready != n_runtimes) && *g == gen,
                                                )
                                                .unwrap();
                                            let mut results = HashMap::new();

                                            if over.load(atomic::Ordering::SeqCst) {
                                                break 'outer;
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);
                                            std::mem::swap(&mut state.2, &mut results);

                                            break (results, state.3.take());
                                        };

                                        let mut resource_idxs = Vec::new();

                                        if errno.is_none() || errno.unwrap() == 0 {
                                            for (i, _result) in function.results.iter().enumerate() {
                                                let mut result_values = Vec::new();
                                                let tdef = function.results.get(i).unwrap().tref.resolve(&spec);

                                                for rt in rts.iter() {
                                                    let results = results.get(rt).unwrap();

                                                    if let Some(results) = results {
                                                        result_values.push(results.get(i).unwrap());
                                                    }
                                                }

                                                let mut rtctxs = rtctxs.write().unwrap();
                                                let ctxs = rtctxs.iter_mut().zip(result_values).collect_vec();
                                                let resource_idx =
                                                    env.write().unwrap().lift_recursively(&spec, ctxs, tdef);

                                                resource_idxs.push(resource_idx);
                                            }
                                        }

                                        lift_results_done_tx.try_send(resource_idxs.clone()).unwrap();

                                        loop {
                                            let (mu, cond) = &*solve_output_contract_init;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(state, Duration::from_millis(100), |(ready, g)| {
                                                    (*ready != n_runtimes) && *g == gen
                                                })
                                                .unwrap();

                                            if over.load(atomic::Ordering::SeqCst) {
                                                tracing::info!("Fuzz over. Stopping thread.");
                                                break 'outer;
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);

                                            break;
                                        }

                                        if errno.is_none() || errno.unwrap() == 0 {
                                            let result_values = results.get(rts.first().unwrap()).unwrap().clone();

                                            strategy
                                                .handle_results(
                                                    &spec,
                                                    function,
                                                    &mut env.write().unwrap(),
                                                    params,
                                                    resource_idxs,
                                                    result_values.as_ref().map(Vec::as_slice),
                                                )
                                                .unwrap();
                                        }

                                        solve_output_contract_done_tx.try_send(()).unwrap();
                                    }

                                    tracing::info!("Strategy thread exiting.");
                                }
                            })?;

                        thread::Builder::new()
                            .name(format!("diff-{run_id}"))
                            .spawn_scoped(scope, {
                                let run = run.clone();
                                let over = over.clone();
                                let cancel = cancel.clone();
                                let diff_init = diff_init.clone();

                                move || -> Result<(), FuzzError> {
                                    run.lock().unwrap().configure_progress_logging(enable_logging);

                                    loop {
                                        let errnos: Vec<_> = loop {
                                            let (mu, cond) = &*diff_init;
                                            let state = mu.lock().unwrap();
                                            let gen = state.1;
                                            let (mut state, result) = cond
                                                .wait_timeout_while(
                                                    state,
                                                    Duration::from_micros(100),
                                                    |(ready, g, _)| *ready != n_runtimes && gen == *g,
                                                )
                                                .unwrap();

                                            if over.load(atomic::Ordering::SeqCst)
                                                || cancel.load(atomic::Ordering::SeqCst)
                                            {
                                                return Ok(());
                                            }

                                            if result.timed_out() {
                                                continue;
                                            }

                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);

                                            break state.2.take().unwrap();
                                        };

                                        let first = errnos.first().unwrap();

                                        for (_runtime_name, errno) in errnos.iter().skip(1) {
                                            match (first.1, errno) {
                                                | (None, None) => continue,
                                                | (None, Some(_)) | (Some(_), None) => {
                                                    tracing::info!("Errno diff found.");
                                                    diff_done_tx.try_send(DiffResult::Errno).unwrap();
                                                    return Ok(());
                                                },
                                                | (Some(l), &Some(r)) => {
                                                    if (l == 0 && r != 0) || (l != 0 && r == 0) {
                                                        tracing::info!("Errno diff found.");
                                                        diff_done_tx.try_send(DiffResult::Errno).unwrap();
                                                        return Ok(());
                                                    }
                                                },
                                            }
                                        }

                                        let run = run.lock().unwrap();
                                        let runtimes = run.runtime_stores().collect::<Vec<_>>();

                                        'outer: for (i, (runtime_0_name, runtime_0)) in runtimes.iter().enumerate() {
                                            let runtime_0 = runtime_0.read().unwrap();
                                            let call_0 = runtime_0.last_call().unwrap();

                                            for j in (i + 1)..runtimes.len() {
                                                let (runtime_1_name, runtime_1) = runtimes.get(j).unwrap();
                                                let runtime_1 = runtime_1.read().unwrap();
                                                let call_1 = runtime_1.last_call().unwrap();

                                                match (call_0.errno, call_1.errno) {
                                                    | (None, None) => {},
                                                    | (Some(errno_0), Some(errno_1))
                                                        if errno_0 == 0 && errno_1 == 0
                                                            || errno_0 != 0 && errno_1 != 0 => {},
                                                    | _ => {
                                                        tracing::error!(
                                                            runtime_a = runtime_0_name,
                                                            runtime_b = runtime_1_name,
                                                            runtime_a_errno = call_0.errno,
                                                            runtime_b_errno = call_1.errno,
                                                            "Errno diff found!"
                                                        );

                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break 'outer;
                                                    },
                                                }

                                                let runtime_0_walk = WalkDir::new(&runtime_0.base_path())
                                                    .sort_by_file_name()
                                                    .min_depth(1)
                                                    .into_iter();
                                                let runtime_1_walk = WalkDir::new(&runtime_1.base_path())
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
                                                                diff_done_tx.try_send(DiffResult::Filesystem).unwrap();
                                                                return Ok(());
                                                            }
                                                        },
                                                        | EitherOrBoth::Left(_) | EitherOrBoth::Right(_) => {
                                                            tracing::error!("Fs diff found.");
                                                            diff_done_tx.try_send(DiffResult::Filesystem).unwrap();
                                                            return Ok(());
                                                        },
                                                    }
                                                }
                                            }
                                        }

                                        diff_done_tx.try_send(DiffResult::Ok).unwrap();
                                    }
                                }
                            })
                            .wrap_err("failed to spawn differ thread")?;

                        let mut runtime_threads = Vec::new();

                        for (i, (runtime_name, store, executor)) in runtimes.into_iter().enumerate() {
                            runtime_threads.push(
                                thread::Builder::new()
                                    .name(format!("drv-{run_id}-{runtime_name}"))
                                    .spawn_scoped(scope, {
                                        let run = run.clone();
                                        let run_id = run_id.clone();
                                        let mmap = mmap.clone();
                                        let over = over.clone();
                                        let cancel = cancel.clone();
                                        let fill_init = fill_init.clone();
                                        let fill_done_rx = fill_done_rx.add_stream();
                                        let diff_init = diff_init.clone();
                                        let diff_done_rx = diff_done_rx.add_stream();
                                        let select_func_init = select_func_init.clone();
                                        let select_func_done_rx = select_func_done_rx.add_stream();
                                        let prep_params_init = prep_params_init.clone();
                                        let prep_params_done_rx = prep_params_done_rx.add_stream();
                                        let lift_results_init = lift_results_init.clone();
                                        let lift_results_done_rx = lift_results_done_rx.add_stream();
                                        let solve_output_contract_init = solve_output_contract_init.clone();
                                        let solve_output_contract_done_rx = solve_output_contract_done_rx.add_stream();
                                        let spec = spec.clone();
                                        let rtctxs = rtctxs.clone();
                                        let runtime_name = runtime_name.clone();

                                        move || -> Result<(), FuzzError> {
                                            run.lock().unwrap().configure_progress_logging(enable_logging);

                                            let mut iteration = 0;
                                            let mut u = Unstructured::new(data);

                                            loop {
                                                if cancel.load(atomic::Ordering::SeqCst) {
                                                    tracing::info!("Cancelling fuzz run.");
                                                    over.store(true, atomic::Ordering::SeqCst);
                                                    return Err(FuzzError::Time);
                                                }

                                                {
                                                    // Let the strategy thread select a function.

                                                    let (mu, cond) = &*select_func_init;
                                                    let mut state = mu.lock().unwrap();

                                                    state.0 += 1;

                                                    if state.0 == n_runtimes {
                                                        cond.notify_all();
                                                    }
                                                }

                                                let function = match select_func_done_rx.recv() {
                                                    | Ok(x) => x,
                                                    | Err(_) => {
                                                        tracing::info!(
                                                            "Strategy thread terminated. Stopping fuzz run."
                                                        );
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                };

                                                tracing::info!(
                                                    run_id = run_id,
                                                    iteration = iteration,
                                                    function = function.name,
                                                    "Calling function."
                                                );
                                                iteration += 1;

                                                {
                                                    let (mu, cond) = &*prep_params_init;
                                                    let mut state = mu.lock().unwrap();

                                                    state.0 += 1;

                                                    if state.0 == n_runtimes {
                                                        // Dispatch to strategy thread to select a function.
                                                        cond.notify_all();
                                                    }
                                                }

                                                let params = match prep_params_done_rx.recv() {
                                                    | Ok(x) => x,
                                                    | Err(_) => {
                                                        tracing::info!(
                                                            "Strategy thread terminated. Stopping fuzz run."
                                                        );
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                };
                                                let (errno, results) = execute_call(
                                                    &spec,
                                                    rtctxs.read().unwrap().get(i).unwrap(),
                                                    &function,
                                                    params.clone(),
                                                    &executor,
                                                )
                                                .unwrap();

                                                {
                                                    let (mu, cond) = &*lift_results_init;
                                                    let mut state = mu.lock().unwrap();

                                                    state.0 += 1;
                                                    state.2.insert(runtime_name.clone(), results.clone());
                                                    state.3 = errno;

                                                    if state.0 == n_runtimes {
                                                        cond.notify_all();
                                                    }
                                                }

                                                let resource_idxs = match lift_results_done_rx.recv() {
                                                    | Ok(x) => x,
                                                    | Err(_) => {
                                                        tracing::info!(
                                                            "Strategy thread terminated. Stopping fuzz run."
                                                        );
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                };

                                                store
                                                    .write()
                                                    .unwrap()
                                                    .record_call(Call {
                                                        function: function.name,
                                                        errno:    errno,
                                                        params:   params
                                                            .iter()
                                                            .map(|p| {
                                                                let (value, resource_idx) = rtctxs
                                                                    .read()
                                                                    .unwrap()
                                                                    .get(i)
                                                                    .unwrap()
                                                                    .lower(p.clone());

                                                                MaybeResourceValue { value, resource_idx }
                                                            })
                                                            .collect_vec(),
                                                        results:  results.map(|results| {
                                                            results
                                                                .iter()
                                                                .zip(resource_idxs)
                                                                .map(|(value, resource_idx)| MaybeResourceValue {
                                                                    value: value.to_owned(),
                                                                    resource_idx,
                                                                })
                                                                .collect_vec()
                                                        }),
                                                    })
                                                    .unwrap();

                                                if u.is_empty() {
                                                    // If the data has been exhausted. Refill it.

                                                    {
                                                        let (mu, cond) = &*fill_init;
                                                        let mut state = mu.lock().unwrap();

                                                        state.0 += 1;

                                                        if state.0 == n_runtimes {
                                                            cond.notify_all();
                                                        }
                                                    }

                                                    fill_done_rx.recv().unwrap();
                                                    data = unsafe {
                                                        std::slice::from_raw_parts(
                                                            mmap.lock().unwrap().as_ptr(),
                                                            BUF_SIZE,
                                                        )
                                                    };
                                                    u = Unstructured::new(data);
                                                }

                                                {
                                                    // Start diff when all runtimes ready.

                                                    let (mu, cond) = &*diff_init;
                                                    let mut state = mu.lock().unwrap();

                                                    state.0 += 1;

                                                    let errno = (runtime_name.clone(), errno);

                                                    match &mut state.2 {
                                                        | Some(results) => results.push(errno),
                                                        | None => state.2 = Some(vec![errno]),
                                                    }

                                                    if state.0 == n_runtimes {
                                                        cond.notify_all();
                                                    }
                                                }

                                                let diff_result = match diff_done_rx.recv() {
                                                    | Ok(result) => result,
                                                    | Err(_) => {
                                                        tracing::info!("Diff thread terminated. Stopping fuzz run.");
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                };

                                                match diff_result {
                                                    | DiffResult::Ok => (),
                                                    | DiffResult::Errno => {
                                                        tracing::info!("Errno diff found. Stopping fuzz run.");
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                    | DiffResult::Filesystem => {
                                                        tracing::info!("Filesystem diff found. Stopping fuzz run.");
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                }
                                                {
                                                    let (mu, cond) = &*solve_output_contract_init;
                                                    let mut state = mu.lock().unwrap();

                                                    state.0 += 1;

                                                    if state.0 == n_runtimes {
                                                        cond.notify_all();
                                                    }
                                                }

                                                match solve_output_contract_done_rx.recv() {
                                                    | Ok(_) => (),
                                                    | Err(_) => {
                                                        tracing::info!("Diff thread terminated. Stopping fuzz run.");
                                                        over.store(true, atomic::Ordering::SeqCst);
                                                        break;
                                                    },
                                                }
                                            }

                                            Ok(())
                                        }
                                    })
                                    .wrap_err(format!("failed to spawn {runtime_name}"))?,
                            );
                        }

                        fill_done_rx.unsubscribe();
                        select_func_done_rx.unsubscribe();
                        prep_params_done_rx.unsubscribe();
                        lift_results_done_rx.unsubscribe();
                        solve_output_contract_done_rx.unsubscribe();
                        diff_done_rx.unsubscribe();

                        Ok(())
                    })
                    .unwrap();

                    run_.lock().unwrap().finish();
                }
            });
        }

        tracing::info!(active_count = pool.active_count(), "Waiting for fuzz runs to complete.");
        pool.join();

        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(self.store.root_path().join("metadata.json"))?,
            &self.store.metadata(),
        )?;

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
enum FuzzError {
    #[error(transparent)]
    Unknown(#[from] eyre::Error),

    #[error("time exceeded")]
    Time,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct FuzzConfig {
    runtimes: Vec<RuntimeFuzzConfig>,
    spec:     PathBuf,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
enum FuzzLoopLimit {
    #[serde(with = "humantime_serde")]
    Time(Duration),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct RuntimeFuzzConfig {
    name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum DiffResult {
    Ok,
    Errno,
    Filesystem,
}
