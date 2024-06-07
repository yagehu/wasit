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
use eyre::{eyre as err, Context as _};
use itertools::{EitherOrBoth, Itertools as _};
use rand::{thread_rng, RngCore};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
use walkdir::WalkDir;
use wazzi_executor::ExecutorRunner;
use wazzi_runners::{Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_specz::{resource::Context, Call, Environment, Resource};
use wazzi_specz_wasi::WasiValue;
use wazzi_store::FuzzStore;

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

    let mut store = FuzzStore::new(Path::new("abc")).wrap_err("failed to init fuzz store")?;
    let mut fuzzer = Fuzzer::new(
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
    let data = fs::read("data")?;

    fuzzer.fuzz(Some(data))?;

    Ok(())
}

#[derive(Debug)]
struct Fuzzer<'s> {
    store:    &'s mut FuzzStore,
    runtimes: Vec<(&'static str, Box<dyn WasiRunner>)>,
}

impl<'s> Fuzzer<'s> {
    pub fn new(
        store: &'s mut FuzzStore,
        runtimes: impl IntoIterator<Item = (&'static str, Box<dyn WasiRunner>)>,
    ) -> Self {
        Self {
            store,
            runtimes: runtimes.into_iter().collect(),
        }
    }

    pub fn fuzz(&mut self, data: Option<Vec<u8>>) -> Result<(), eyre::Error> {
        let mut run_store = self.store.new_run()?;
        let mut env = Environment::preview1()?;
        let fdflags = env
            .spec()
            .types
            .get(*env.spec().types_map.get("fdflags").unwrap())
            .unwrap()
            .wasi
            .flags()
            .unwrap();
        let filetype = env
            .spec()
            .types
            .get(*env.spec().types_map.get("filetype").unwrap())
            .unwrap()
            .wasi
            .variant()
            .unwrap();
        let resource_id = env.new_resource(
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
        let env = Arc::new(RwLock::new(env));
        let data = match data {
            | Some(d) => d,
            | None => {
                let mut data = vec![0u8; 65536];

                thread_rng().fill_bytes(&mut data);

                data
            },
        };

        run_store.write_data(&data)?;

        thread::scope(|scope| -> Result<_, eyre::Error> {
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
                            let mut store = run_store.new_runtime(runtime_name.to_string(), "-")?;
                            let tx = tx.clone();
                            let cancel = cancel.clone();
                            let pause_pair = pause_pair.clone();
                            let resume_pair = resume_pair.clone();
                            let n_live_threads = n_live_threads.clone();

                            move || -> Result<(), eyre::Error> {
                                let stderr = fs::OpenOptions::new()
                                    .write(true)
                                    .create_new(true)
                                    .open(store.path.join("stderr"))?;
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
                                let mut ctx = Context::new();

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
                                        panic!("cancelled");
                                    }

                                    if u.is_empty() {
                                        panic!("data exhausted");
                                    }

                                    let (function, ok, _results) = env
                                        .read()
                                        .unwrap()
                                        .call_arbitrary_function(
                                            &mut u,
                                            &mut ctx,
                                            &executor,
                                            store.trace_mut(),
                                        )
                                        .unwrap();
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
                                                .filter(|result| !result.ty.attributes.is_empty())
                                                .map(|result| {
                                                    (
                                                        result.name.clone(),
                                                        env.write().unwrap().new_resource(
                                                            result
                                                                .ty
                                                                .name
                                                                .as_ref()
                                                                .unwrap()
                                                                .to_string(),
                                                            Resource {
                                                                attributes: result
                                                                    .ty
                                                                    .attributes
                                                                    .iter()
                                                                    .map(|(name, ty)| {
                                                                        (
                                                                            name.clone(),
                                                                            ty.wasi.zero_value(),
                                                                        )
                                                                    })
                                                                    .collect(),
                                                            },
                                                        ),
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
                    move || -> Result<(), eyre::Error> {
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
                                                        && fs::read(a.path())?
                                                            != fs::read(b.path())?)
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

            Ok(())
        })?;

        Err(err!("fuzz loop ended"))
    }
}
