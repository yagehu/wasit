use std::{
    collections::{HashMap, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use arbitrary::Unstructured;
use eyre::{eyre as err, Context as _};
use rand::{thread_rng, RngCore};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
use wazzi_executor::ExecutorRunner;
use wazzi_runners::{Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_specz::{resource::Context, Environment, Resource};
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

    fuzzer.fuzz()?;

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

    pub fn fuzz(&mut self) -> Result<(), eyre::Error> {
        let mut run_store = self.store.new_run()?;
        let mut env = Environment::preview1()?;
        let fdflags = env
            .spec()
            .types
            .get("fdflags")
            .unwrap()
            .wasi
            .flags()
            .unwrap();
        let filetype = env
            .spec()
            .types
            .get("filetype")
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
                        "file-type".to_owned(),
                        filetype.value_from_name("directory", None).unwrap(),
                    ),
                ]),
            },
        );
        let mut data = vec![0u8; 4096];
        let env = Arc::new(env);

        thread_rng().fill_bytes(&mut data);

        thread::scope(|scope| -> Result<_, eyre::Error> {
            for (runtime_name, runtime) in &self.runtimes {
                thread::Builder::new()
                    .name(runtime_name.to_string())
                    .spawn_scoped(scope, {
                        let mut u = Unstructured::new(&data);
                        let env = env.clone();
                        let store = run_store.new_runtime(runtime_name.to_string(), "-")?;

                        move || -> Result<_, eyre::Error> {
                            let mut ctx = Context::new();

                            ctx.resources
                                .insert(resource_id, WasiValue::Handle(runtime.base_dir_fd()));

                            let stderr = fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(store.path.join("stderr"))?;
                            let executor = ExecutorRunner::new(
                                runtime.as_ref(),
                                PathBuf::from("target/debug/wazzi-executor-pb.wasm")
                                    .canonicalize()
                                    .unwrap(),
                                store.base.clone(),
                                Some(store.base.clone()),
                            )
                            .run(Arc::new(Mutex::new(stderr)))
                            .unwrap();

                            loop {
                                env.call(&mut u, &mut ctx, &executor, "path_open")?;
                            }

                            Ok(())
                        }
                    })
                    .wrap_err(format!("failed to spawn {runtime_name}"))?;
            }

            Ok(())
        })?;

        Err(err!("fuzz loop ended"))
    }
}
