use std::{
    collections::{HashMap, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{self, AtomicBool, AtomicUsize},
        mpsc,
        Arc,
        Condvar,
        Mutex,
        RwLock,
    },
    thread,
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
use wazzi_executor::ExecutorRunner;
use wazzi_runners::{Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_specz::{
    function_picker::{resource::ResourcePicker, solver::SolverPicker, FunctionPicker},
    param_generator::{
        stateful::StatefulParamsGenerator,
        stateless::StatelessParamsGenerator,
        ParamsGenerator,
    },
    preview1::{
        spec::{Spec, WasiValue},
        witx,
    },
    resource::Context,
    Call,
    Environment,
    Resource,
};
use wazzi_store::FuzzStore;

#[derive(Parser, Debug)]
struct Cmd {
    #[arg(long)]
    data: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = FunctionPickerType::Stateful)]
    function_picker: FunctionPickerType,

    #[arg(long, value_enum, default_value_t = ParamsGeneratorType::Stateful)]
    params_generator: ParamsGeneratorType,

    #[arg(long)]
    max_epochs: Option<usize>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum FunctionPickerType {
    Stateless,
    Stateful,
}

impl From<FunctionPickerType> for Arc<dyn FunctionPicker + Send + Sync> {
    fn from(value: FunctionPickerType) -> Self {
        match value {
            | FunctionPickerType::Stateless => Arc::new(ResourcePicker),
            | FunctionPickerType::Stateful => Arc::new(SolverPicker),
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum ParamsGeneratorType {
    Stateless,
    Stateful,
}

impl From<ParamsGeneratorType> for Arc<dyn ParamsGenerator + Send + Sync> {
    fn from(value: ParamsGeneratorType) -> Self {
        match value {
            | ParamsGeneratorType::Stateless => Arc::new(StatelessParamsGenerator),
            | ParamsGeneratorType::Stateful => Arc::new(StatefulParamsGenerator),
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

    let cmd = Cmd::parse();
    let mut store = FuzzStore::new(Path::new("abc")).wrap_err("failed to init fuzz store")?;
    let mut fuzzer = Fuzzer::new(
        cmd.function_picker.into(),
        cmd.params_generator.into(),
        &mut store,
        [
            (
                "node",
                Box::new(Node::new(Path::new("node"))) as Box<dyn WasiRunner>,
            ),
            ("wamr", Box::new(Wamr::new(Path::new("iwasm")))),
            ("wasmedge", Box::new(Wasmedge::new(Path::new("wasmedge")))),
            ("wasmer", Box::new(Wasmer::new(Path::new("wasmer")))),
            ("wasmtime", Box::new(Wasmtime::new(Path::new("wasmtime")))),
            ("wazero", Box::new(Wazero::new(Path::new("wazero")))),
        ],
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
    function_picker:  Arc<dyn FunctionPicker + Send + Sync>,
    params_generator: Arc<dyn ParamsGenerator + Send + Sync>,
    store:            &'s mut FuzzStore,
    runtimes:         Vec<(&'static str, Box<dyn WasiRunner>)>,
}

impl<'s> Fuzzer<'s> {
    pub fn new(
        function_picker: Arc<dyn FunctionPicker + Send + Sync>,
        params_generator: Arc<dyn ParamsGenerator + Send + Sync>,
        store: &'s mut FuzzStore,
        runtimes: impl IntoIterator<Item = (&'static str, Box<dyn WasiRunner>)>,
    ) -> Self {
        Self {
            function_picker,
            params_generator,
            store,
            runtimes: runtimes.into_iter().collect(),
        }
    }

    pub fn fuzz_loop(
        &mut self,
        data: Option<Vec<u8>>,
        max_epochs: Option<usize>,
    ) -> Result<(), eyre::Error> {
        let mut data = data;
        let mut epoch = 0;

        loop {
            if let Some(max_epochs) = max_epochs {
                if epoch == max_epochs {
                    break;
                }
            }

            epoch += 1;

            match self.fuzz(epoch, data.take()) {
                | Ok(_) => continue,
                | Err(FuzzError::DiffFound) => continue,
                | Err(err) => return Err(err!(err)),
            }
        }

        Ok(())
    }

    pub fn fuzz(&mut self, epoch: usize, data: Option<Vec<u8>>) -> Result<(), FuzzError> {
        let mut run_store = self
            .store
            .new_run()
            .wrap_err("failed to init new run store")?;
        let env = Environment::preview1()?;
        let env = Arc::new(RwLock::new(env));
        let data = match data {
            | Some(d) => d,
            | None => {
                let mut data = vec![0u8; 131072];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };
        let z3_cfg = z3::Config::new();
        let z3_ctx = z3::Context::new(&z3_cfg);
        let mut spec = Spec::new(&z3_ctx);

        witx::preview1(&z3_ctx, &mut spec).unwrap();

        let fdflags = spec.get_type_def("fdflags").unwrap().wasi.flags().unwrap();
        let filetype = spec
            .get_type_def("filetype")
            .unwrap()
            .wasi
            .variant()
            .unwrap();
        let resource_id = env.write().unwrap().new_resource(
            "fd".to_owned(),
            Resource {
                attributes: HashMap::from([
                    ("offset".to_owned(), WasiValue::U64(0)),
                    ("flags".to_owned(), fdflags.value(HashSet::new())),
                    (
                        "type".to_owned(),
                        filetype.value_from_name("directory", None).unwrap(),
                    ),
                ]),
            },
        );

        run_store
            .write_data(&data)
            .wrap_err("failed to write data")?;

        thread::scope(|scope| -> Result<_, FuzzError> {
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let pause_pair = Arc::new((Mutex::new((0, 0usize)), Condvar::new()));
            let resume_pair = Arc::new((Mutex::new((true, 0usize)), Condvar::new()));
            let n_live_threads = Arc::new(AtomicUsize::new(self.runtimes.len()));
            let mut runtime_threads = Vec::with_capacity(self.runtimes.len());

            for (runtime_name, runtime) in &self.runtimes {
                runtime_threads.push(
                    thread::Builder::new()
                        .name(runtime_name.to_string())
                        .spawn_scoped(scope, {
                            let mut u = Unstructured::new(&data);
                            let env = env.clone();
                            let mut store = run_store
                                .new_runtime(runtime_name.to_string(), "-")
                                .wrap_err("failed to init runtime store")?;
                            let tx = tx.clone();
                            let cancel = cancel.clone();
                            let pause_pair = pause_pair.clone();
                            let resume_pair = resume_pair.clone();
                            let n_live_threads = n_live_threads.clone();
                            let function_picker = self.function_picker.clone();
                            let params_generator = self.params_generator.clone();

                            move || -> Result<(), FuzzError> {
                                let z3_cfg = z3::Config::new();
                                let z3_ctx = z3::Context::new(&z3_cfg);
                                let mut spec = Spec::new(&z3_ctx);

                                witx::preview1(&z3_ctx, &mut spec).unwrap();

                                let interface = spec
                                    .interfaces
                                    .get_by_key("wasi_snapshot_preview1")
                                    .unwrap();
                                let stderr = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(store.path.join("stderr"))
                                    .wrap_err("failed to open stderr file")?;
                                let (executor, prefix) = ExecutorRunner::new(
                                    runtime.as_ref(),
                                    PathBuf::from("target/debug/wazzi-executor-pb.wasm")
                                        .canonicalize()
                                        .unwrap(),
                                    store.path.clone(),
                                    Some(store.base.clone()),
                                )
                                .run(Arc::new(Mutex::new(stderr)))
                                .unwrap();
                                let mut ctx = Context::new(store.base.clone());
                                let mut iteration = 0;

                                ctx.resources.insert(
                                    resource_id,
                                    (WasiValue::Handle(runtime.base_dir_fd()), prefix),
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

                                    if cancel.load(atomic::Ordering::SeqCst) {
                                        return Err(FuzzError::DiffFound);
                                    }

                                    if u.is_empty() {
                                        panic!("data exhausted");
                                    }

                                    let function = function_picker.pick_function(
                                        &mut u,
                                        interface,
                                        &env.read().unwrap(),
                                        &ctx,
                                        &spec,
                                    )?;

                                    tracing::info!(
                                        epoch = epoch,
                                        iteration = iteration,
                                        function = function.name,
                                        "Calling function."
                                    );
                                    iteration += 1;

                                    let (ok, _results) = match env.read().unwrap().call(
                                        &mut u,
                                        &mut ctx,
                                        &spec,
                                        &executor,
                                        store.trace_mut(),
                                        function,
                                        &*params_generator,
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

                                        let result_resources = if ok {
                                            function
                                                .results
                                                .iter()
                                                .filter_map(|result| {
                                                    result.tref.resource_type_def(&spec).map(
                                                        |tdef| {
                                                            (
                                                                result.name.clone(),
                                                                env.write().unwrap().new_resource(
                                                                    tdef.name.clone(),
                                                                    Resource {
                                                                        attributes: tdef
                                                                            .attributes
                                                                            .as_ref()
                                                                            .unwrap()
                                                                            .iter()
                                                                            .map(|(name, tref)| {
                                                                                (
                                                                                    name.clone(),
                                                                                    tref.wasi_type(
                                                                                        &spec,
                                                                                    )
                                                                                    .zero_value(
                                                                                        &spec,
                                                                                    ),
                                                                                )
                                                                            })
                                                                            .collect(),
                                                                    },
                                                                ),
                                                            )
                                                        },
                                                    )
                                                })
                                                .collect::<HashMap<_, _>>()
                                        } else {
                                            HashMap::new()
                                        };

                                        tx.send((function, ok, result_resources)).unwrap();
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
                    move || -> Result<(), FuzzError> {
                        while let Ok((function, ok, result_resources)) = rx.recv() {
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

                            if ok {
                                env.write()
                                    .unwrap()
                                    .execute_function_effects(&function, result_resources)
                                    .unwrap();
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
