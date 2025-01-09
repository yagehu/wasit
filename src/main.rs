#![feature(trait_upcasting)]

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{self, stderr, IsTerminal},
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
use memmap::MmapOptions;
use multiqueue::broadcast_queue;
use rand::{thread_rng, RngCore};
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
use walkdir::WalkDir;
use wazzi::{
    apply_env_initializers,
    execute_call,
    normalization::InitializeState,
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
use wazzi_store::{FuzzStore, RuntimeStore};

#[derive(Parser, Debug)]
struct Cmd {
    #[arg()]
    config: PathBuf,

    #[arg()]
    path: PathBuf,

    #[arg(long, value_enum, default_value_t = Strategy::Stateful)]
    strategy: Strategy,
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
                    .with_env_var("WAZZI_LOG_LEVEL")
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(ErrorLayer::default())
            .with(subscriber),
    )
    .wrap_err("failed to configure tracing")?;

    let orig_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    let cmd = Cmd::parse();
    let config: FuzzConfig = serde_yml::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(&cmd.config)
            .wrap_err("failed to read fuzz config")?,
    )
    .wrap_err("failed to deserialize fuzz config")?;
    let mut store = FuzzStore::new(&cmd.path).wrap_err("failed to init fuzz store")?;
    let mut runtimes = Vec::with_capacity(config.runtimes.len());

    for runtime in config.runtimes {
        let rt = match runtime.name.as_str() {
            | "node" => Box::new(Node::default()) as Box<dyn InitializeState>,
            | "wamr" => Box::new(Wamr::default()) as Box<dyn InitializeState>,
            | "wasmedge" => Box::new(Wasmedge::default()) as Box<dyn InitializeState>,
            | "wasmer" => Box::new(Wasmer::default()) as Box<dyn InitializeState>,
            | "wasmtime" => Box::new(Wasmtime::default()) as Box<dyn InitializeState>,
            | "wazero" => Box::new(Wazero::default()) as Box<dyn InitializeState>,
            | name @ _ => return Err(err!("unknown runtime {name}")),
        };

        runtimes.push((runtime.name, rt));
    }

    let mut fuzzer = Fuzzer::new(
        fs::read_to_string(config.spec).wrap_err("failed to read spec file")?,
        cmd.strategy,
        &mut store,
        runtimes,
    );

    fuzzer.fuzz_loop(config.limit)?;

    Ok(())
}

#[derive(Debug)]
struct Fuzzer<'s> {
    spec:     String,
    strategy: Strategy,
    store:    &'s mut FuzzStore,
    runtimes: Vec<(String, Box<dyn InitializeState>)>,
}

impl<'s> Fuzzer<'s> {
    pub fn new(
        spec: String,
        strategy: Strategy,
        store: &'s mut FuzzStore,
        runtimes: impl IntoIterator<Item = (String, Box<dyn InitializeState>)>,
    ) -> Self {
        Self {
            spec,
            strategy,
            store,
            runtimes: runtimes.into_iter().collect(),
        }
    }

    pub fn fuzz_loop(&mut self, limit: Option<FuzzLoopLimit>) -> Result<(), eyre::Error> {
        let mut epoch_idx = 0;
        let time_cancel = Arc::new(AtomicBool::new(false));
        let data_files = match &limit {
            | None => None,
            | Some(limit) => match limit {
                | FuzzLoopLimit::Time(duration) => {
                    thread::Builder::new()
                        .name("timer".to_owned())
                        .spawn({
                            let time_cancel = time_cancel.clone();
                            let duration = duration.clone();

                            move || {
                                thread::sleep(duration);
                                time_cancel.store(true, atomic::Ordering::SeqCst);
                                tracing::warn!(
                                    duration = humantime::Duration::from(duration).to_string(),
                                    "Time's up. Cancelling."
                                );
                            }
                        })
                        .wrap_err("failed to spawn timer thread")?;

                    None
                },
                | FuzzLoopLimit::Epochs(epochs) => match epochs.get(epoch_idx) {
                    | None => return Ok(()),
                    | Some(cfg) => Some(&cfg.data_files),
                },
            },
        };

        loop {
            if time_cancel.load(atomic::Ordering::SeqCst) {
                tracing::info!("Fuzz loop cancelled.");
                break;
            }

            let result = self.fuzz(
                epoch_idx,
                time_cancel.clone(),
                data_files.as_ref().map(AsRef::as_ref),
            );

            epoch_idx += 1;

            match result {
                | Ok(_) => continue,
                | Err(FuzzError::DiffFound) => continue,
                | Err(FuzzError::Time) => break,
                | Err(err) => return Err(err!(err)),
            }
        }

        Ok(())
    }

    pub fn fuzz(
        &mut self,
        epoch: usize,
        time_cancel: Arc<AtomicBool>,
        data_files: Option<&[PathBuf]>,
    ) -> Result<(), FuzzError> {
        const BUF_SIZE: usize = 131072;

        let run_store = Arc::new(Mutex::new(
            self.store
                .new_run()
                .wrap_err("failed to init new run store")?,
        ));
        let data_file_idx = data_files.map(|_| Arc::new(AtomicUsize::new(0)));
        let spec = Spec::preview1(&self.spec).wrap_err("failed to init spec")?;
        let mut initializers: Vec<(String, EnvironmentInitializer)> = Default::default();
        let mut runtimes: Vec<_> = Default::default();

        for (runtime_name, runtime) in &self.runtimes {
            let store: RuntimeStore<Call> = run_store
                .lock()
                .unwrap()
                .new_runtime(runtime_name.to_string(), "-")
                .wrap_err("failed to init runtime store")?;
            let stderr = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(store.path.join("stderr"))
                .wrap_err("failed to open stderr file")?;
            let executor = RunningExecutor::from_wasi_runner(
                runtime.as_ref(),
                Path::new("target")
                    .join("debug")
                    .join("wazzi-executor.wasm")
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

            initializers.push((runtime_name.to_string(), initializer));
            runtimes.push((runtime_name.to_string(), store, executor));
        }

        let rts = initializers
            .iter()
            .map(|(name, _)| name.to_string())
            .collect_vec();
        let (env, rtctxs, preopens) =
            apply_env_initializers(&spec, &initializers.into_iter().map(|p| p.1).collect_vec());
        let env = Arc::new(RwLock::new(env));
        let rtctxs = Arc::new(RwLock::new(rtctxs));
        let mut mmap = MmapOptions::new().len(BUF_SIZE).map_anon().unwrap();
        let buf_ptr = mmap.as_ptr();

        #[derive(Copy, Clone)]
        struct ShareablePtr(*const u8);

        unsafe impl Send for ShareablePtr {
        }

        let buf_ptr = ShareablePtr(buf_ptr);

        thread::scope(|scope| -> Result<_, FuzzError> {
            let diff_init_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (diff_tx, diff_rx) = mpsc::channel();
            let diff_done_pair = Arc::new((Mutex::new((false, 0usize)), Condvar::new()));
            let select_func_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let prep_params_init_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let lift_results_init_pair = Arc::new((
                Mutex::new((
                    0,
                    0usize,
                    HashMap::<String, Option<Vec<WasiValue>>>::new(),
                    None::<i32>,
                )),
                Condvar::new(),
            ));
            let solve_output_contract_init_pair =
                Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let (fill_tx, fill_rx) = mpsc::channel();
            let filled_pair = Arc::new((Mutex::new((false, 0usize)), Condvar::new()));
            let mut runtime_threads = Vec::with_capacity(self.runtimes.len());
            let diff_cancel = Arc::new(AtomicBool::new(false));
            let n_runtimes = self.runtimes.len();
            let (select_func_done_tx, select_func_done_rx) = broadcast_queue(1);
            let (prep_params_done_tx, prep_params_done_rx) = broadcast_queue(1);
            let (lift_results_done_tx, lift_results_done_rx) = broadcast_queue(1);
            let (solve_output_contract_done_tx, solve_output_contract_done_rx) = broadcast_queue(1);

            thread::Builder::new()
                .name("buf-filler".to_string())
                .spawn_scoped(scope, {
                    let filled_pair = filled_pair.clone();
                    let run_store = run_store.clone();
                    let data_file_idx = data_file_idx.clone();

                    move || {
                        while let Ok(()) = fill_rx.recv() {
                            match data_file_idx {
                                | None => {
                                    thread_rng().fill_bytes(&mut mmap);
                                },
                                | Some(ref idx) => {
                                    let data_file_path = data_files
                                        .unwrap()
                                        .get(idx.fetch_add(1, atomic::Ordering::SeqCst))
                                        .unwrap();
                                    let data = fs::read(data_file_path).unwrap();

                                    mmap.copy_from_slice(&data);
                                },
                            }

                            run_store.lock().unwrap().write_data(&mmap).unwrap();

                            {
                                let (mu, cond) = &*filled_pair;
                                let mut state = mu.lock().unwrap();

                                state.0 = true;
                                state.1 = state.1.wrapping_add(1);
                                cond.notify_all();
                            }
                        }
                    }
                })
                .wrap_err("failed to spawn buf-filler thread")?;

            let mut data = {
                let (mu, cond) = &*filled_pair;
                let mut state = mu.lock().unwrap();
                let gen = state.1;

                state.0 = false;
                fill_tx.send(()).unwrap();
                drop(
                    cond.wait_while(state, |(filled, g)| !*filled && *g == gen)
                        .unwrap(),
                );

                unsafe { std::slice::from_raw_parts(buf_ptr.0, BUF_SIZE) }
            };

            thread::Builder::new()
                .name("strategy".to_string())
                .spawn_scoped(scope, {
                    let env = env.clone();
                    let strategy = self.strategy;
                    let select_func_pair = select_func_pair.clone();
                    let prep_params_init_pair = prep_params_init_pair.clone();
                    let lift_results_init_pair = lift_results_init_pair.clone();
                    let solve_output_contract_init_pair = solve_output_contract_init_pair.clone();
                    let rtctxs = rtctxs.clone();

                    move || {
                        let mut u = Unstructured::new(data);
                        let cfg = z3::Config::new();
                        let ctx = z3::Context::new(&cfg);
                        let mut strategy = strategy.into_call_strategy(&mut u, &ctx, preopens);

                        loop {
                            {
                                let (mu, cond) = &*select_func_pair;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let mut state = cond
                                    .wait_while(state, |(select, g)| {
                                        *select != n_runtimes && *g == gen
                                    })
                                    .unwrap();

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                            }

                            let function = strategy
                                .select_function(&spec, &env.read().unwrap())
                                .unwrap();

                            select_func_done_tx.try_send(function.to_owned()).unwrap();

                            {
                                let (mu, cond) = &*prep_params_init_pair;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let mut state = cond
                                    .wait_while(state, |(ready, g)| {
                                        (*ready != n_runtimes) && *g == gen
                                    })
                                    .unwrap();

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                            }

                            let params = strategy
                                .prepare_arguments(&spec, function, &env.read().unwrap())
                                .unwrap();

                            prep_params_done_tx.try_send(params.clone()).unwrap();

                            let (results, errno) = {
                                let (mu, cond) = &*lift_results_init_pair;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let mut state = cond
                                    .wait_while(state, |(ready, g, _results, _errno)| {
                                        (*ready != n_runtimes) && *g == gen
                                    })
                                    .unwrap();
                                let mut results = HashMap::new();

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                                std::mem::swap(&mut state.2, &mut results);

                                (results, state.3.take())
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

                            lift_results_done_tx
                                .try_send(resource_idxs.clone())
                                .unwrap();

                            {
                                let (mu, cond) = &*solve_output_contract_init_pair;
                                let state = mu.lock().unwrap();
                                let gen = state.1;
                                let mut state = cond
                                    .wait_while(state, |(ready, g)| {
                                        (*ready != n_runtimes) && *g == gen
                                    })
                                    .unwrap();

                                state.0 = 0;
                                state.1 = state.1.wrapping_add(1);
                            }

                            if errno.is_none() || errno.unwrap() == 0 {
                                strategy
                                    .handle_results(
                                        &spec,
                                        function,
                                        &mut env.write().unwrap(),
                                        params,
                                        resource_idxs,
                                    )
                                    .unwrap();
                            }

                            solve_output_contract_done_tx.try_send(()).unwrap();
                        }
                    }
                })
                .wrap_err("failed to spawn strategy thread")?;

            for (i, (runtime_name, mut store, executor)) in runtimes.into_iter().enumerate() {
                runtime_threads.push(
                    thread::Builder::new()
                        .name(runtime_name.to_string())
                        .spawn_scoped(scope, {
                            let env = env.clone();
                            let fill_tx = fill_tx.clone();
                            let diff_tx = diff_tx.clone();
                            let diff_cancel = diff_cancel.clone();
                            let time_cancel = time_cancel.clone();
                            let diff_init_pair = diff_init_pair.clone();
                            let diff_done_pair = diff_done_pair.clone();
                            let filled_pair = filled_pair.clone();
                            let select_func_pair = select_func_pair.clone();
                            let select_func_done_rx = select_func_done_rx.add_stream();
                            let prep_params_init_pair = prep_params_init_pair.clone();
                            let prep_params_done_rx = prep_params_done_rx.add_stream();
                            let lift_results_init_pair = lift_results_init_pair.clone();
                            let lift_results_done_rx = lift_results_done_rx.add_stream();
                            let solve_output_contract_init_pair =
                                solve_output_contract_init_pair.clone();
                            let solve_output_contract_done_rx =
                                solve_output_contract_done_rx.add_stream();
                            let spec = self.spec.clone();
                            let rtctxs = rtctxs.clone();
                            let runtime_name = runtime_name.clone();

                            move || -> Result<(), FuzzError> {
                                let spec = Spec::preview1(&spec).unwrap();
                                let mut iteration = 0;
                                let buf_ptr = buf_ptr;
                                let buf_ptr = buf_ptr.0;
                                let mut u = Unstructured::new(data);

                                loop {
                                    if diff_cancel.load(atomic::Ordering::SeqCst) {
                                        let f = fs::OpenOptions::new()
                                            .create_new(true)
                                            .write(true)
                                            .open(&store.path.join("env.json"))
                                            .unwrap();

                                        serde_json::to_writer_pretty(f, &*env.read().unwrap())
                                            .unwrap();

                                        return Err(FuzzError::DiffFound);
                                    }

                                    if time_cancel.load(atomic::Ordering::SeqCst) {
                                        return Err(FuzzError::Time);
                                    }

                                    let env = env.clone();

                                    if diff_cancel.load(atomic::Ordering::SeqCst) {
                                        let f = fs::OpenOptions::new()
                                            .create_new(true)
                                            .write(true)
                                            .open(&store.path.join("env.json"))
                                            .unwrap();

                                        serde_json::to_writer_pretty(f, &*env.read().unwrap())
                                            .unwrap();

                                        return Err(FuzzError::DiffFound);
                                    }

                                    if time_cancel.load(atomic::Ordering::SeqCst) {
                                        return Err(FuzzError::Time);
                                    }

                                    {
                                        let (mu, cond) = &*select_func_pair;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            // Dispatch to strategy thread to select a function.
                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);
                                            cond.notify_all();
                                        }
                                    }

                                    let function = select_func_done_rx.recv().unwrap();

                                    tracing::info!(
                                        epoch = epoch,
                                        iteration = iteration,
                                        function = function.name,
                                        "Calling function."
                                    );
                                    iteration += 1;

                                    {
                                        let (mu, cond) = &*prep_params_init_pair;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            // Dispatch to strategy thread to select a function.
                                            cond.notify_all();
                                        }
                                    }

                                    let params = prep_params_done_rx.recv().unwrap();
                                    let (errno, results) = match execute_call(
                                        &spec,
                                        rtctxs.read().unwrap().get(i).unwrap(),
                                        store.trace_mut(),
                                        &function,
                                        params.clone(),
                                        &executor,
                                    ) {
                                        | Ok(results) => results,
                                        | Err(err) => {
                                            diff_cancel.store(true, atomic::Ordering::SeqCst);
                                            return Err(FuzzError::Unknown(err));
                                        },
                                    };

                                    {
                                        let (mu, cond) = &*lift_results_init_pair;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;
                                        state.2.insert(runtime_name.clone(), results.clone());
                                        state.3 = errno;

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    let resource_idxs = lift_results_done_rx.recv().unwrap();

                                    store
                                        .trace_mut()
                                        .end_call(&Call {
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

                                                    MaybeResourceValue {
                                                        value,
                                                        resource_idx,
                                                    }
                                                })
                                                .collect_vec(),
                                            results:  results.map(|results| {
                                                results
                                                    .iter()
                                                    .zip(resource_idxs)
                                                    .map(|(value, resource_idx)| {
                                                        MaybeResourceValue {
                                                            value: value.to_owned(),
                                                            resource_idx,
                                                        }
                                                    })
                                                    .collect_vec()
                                            }),
                                        })
                                        .unwrap();

                                    {
                                        let (mu, cond) = &*solve_output_contract_init_pair;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            cond.notify_all();
                                        }
                                    }

                                    {
                                        // Start diff when all runtimes ready.

                                        let (mu, cond) = &*diff_init_pair;
                                        let mut state = mu.lock().unwrap();

                                        state.0 += 1;

                                        if state.0 == n_runtimes {
                                            if u.is_empty() {
                                                let (mu, cond) = &*filled_pair;
                                                let mut state = mu.lock().unwrap();
                                                let gen = state.1;

                                                state.0 = false;
                                                fill_tx.send(()).unwrap();

                                                drop(
                                                    cond.wait_while(state, |(filled, g)| {
                                                        !*filled && gen == *g
                                                    })
                                                    .unwrap(),
                                                );

                                                data = unsafe {
                                                    std::slice::from_raw_parts(buf_ptr, BUF_SIZE)
                                                };
                                                u = Unstructured::new(data);
                                            }

                                            diff_done_pair.0.lock().unwrap().0 = false;
                                            state.0 = 0;
                                            state.1 = state.1.wrapping_add(1);
                                            cond.notify_all();
                                            diff_tx.send(()).unwrap();
                                        }
                                    }

                                    {
                                        // Wait for diff to complete.

                                        let (mu, cond) = &*diff_done_pair;
                                        let state = mu.lock().unwrap();
                                        let gen = state.1;

                                        drop(
                                            cond.wait_while(state, |(done, g)| !*done && gen == *g)
                                                .unwrap(),
                                        );
                                    }

                                    solve_output_contract_done_rx.recv().unwrap();
                                }
                            }
                        })
                        .wrap_err(format!("failed to spawn {runtime_name}"))?,
                );
            }

            select_func_done_rx.unsubscribe();
            prep_params_done_rx.unsubscribe();
            lift_results_done_rx.unsubscribe();
            solve_output_contract_done_rx.unsubscribe();

            thread::Builder::new()
                .name("diff".to_string())
                .spawn_scoped(scope, {
                    let cancel = diff_cancel.clone();

                    move || -> Result<(), FuzzError> {
                        while let Ok(()) = diff_rx.recv() {
                            let runtimes = run_store
                                .lock()
                                .unwrap()
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

                            {
                                let (mu, cond) = &*diff_done_pair;
                                let mut state = mu.lock().unwrap();

                                state.0 = true;
                                state.1 = state.1.wrapping_add(1);
                                cond.notify_all();
                            }
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

    #[error("time exceeded")]
    Time,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct FuzzConfig {
    runtimes: Vec<RuntimeFuzzConfig>,
    spec:     PathBuf,
    limit:    Option<FuzzLoopLimit>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
enum FuzzLoopLimit {
    #[serde(with = "humantime_serde")]
    Time(Duration),

    Epochs(Vec<EpochConfig>),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct EpochConfig {
    data_files: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct RuntimeFuzzConfig {
    name: String,
}
