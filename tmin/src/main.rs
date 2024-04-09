use std::{
    fs,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use petgraph::stable_graph::StableDiGraph;
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt as _;
use wazzi_fuzzer::{Fuzzer, Runtime};
use wazzi_runners::{Node, Wamr, Wasmedge, Wasmer, Wasmtime};
use wazzi_store::{Action, FuzzStore, RunStore};
use wazzi_wazi::seed::Seed;

#[derive(Parser, Clone, Debug)]
struct Cmd {
    #[arg()]
    path: PathBuf,

    #[arg()]
    seed: PathBuf,

    #[arg()]
    workspace: PathBuf,
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::Registry::default()
            .with(ErrorLayer::default())
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(io::stderr)
                    .pretty(),
            ),
    )
    .wrap_err("failed to configure tracing")?;

    let cmd = Cmd::parse();
    let run_store = RunStore::resume(&cmd.path)?;
    let data = run_store.data().wrap_err("failed to read data")?;
    let runtime_store = run_store.runtimes()?.next().unwrap();
    let mut calls = Vec::new();

    tracing::info!("Using runtime {}", runtime_store.name());

    for action_store in runtime_store.trace().actions()? {
        let action = action_store.read().wrap_err("failed to read action")?;

        if let Action::Call(call) = action {
            calls.push(call);
        }
    }

    let runtimes_dir = PathBuf::from("runtimes");
    let seed: Seed = serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(&cmd.seed)
            .wrap_err("failed to open seed file")?,
    )
    .wrap_err("failed to deserialize seed")?;

    tracing::info!("Found {} calls.", calls.len());
    tracing::debug!("Sanity test.");

    let mut fuzzer = Fuzzer::new(
        seed,
        [
            Runtime {
                name:   "node",
                repo:   runtimes_dir.join("node"),
                runner: Arc::new(Node::new(Path::new("node"))),
            },
            Runtime {
                name:   "wamr",
                repo:   runtimes_dir.join("wasm-micro-runtime"),
                runner: Arc::new(Wamr::new(Path::new("iwasm"))),
            },
            Runtime {
                name:   "wasmedge",
                repo:   runtimes_dir.join("WasmEdge"),
                runner: Arc::new(Wasmedge::new(Path::new("wasmedge"))),
            },
            Runtime {
                name:   "wasmer",
                repo:   runtimes_dir.join("wasmer"),
                runner: Arc::new(Wasmer::new(Path::new("wasmer"))),
            },
            Runtime {
                name:   "wasmtime",
                repo:   runtimes_dir.join("wasmtime"),
                runner: Arc::new(Wasmtime::new(Path::new("wasmtime"))),
            },
        ],
    );
    let mut fuzz_store = FuzzStore::new(&cmd.workspace.join("sanity"))
        .wrap_err("failed to init sanity fuzz store")?;

    // fuzzer
    //     .fuzz_loop(&mut fuzz_store, Some(&data))
    //     .wrap_err("failed to sanity fuzz")?;

    // let mut graph = StableDiGraph::new();

    // for call in &calls {
    //     graph.add_node(call.clone());
    // }

    Ok(())
}
