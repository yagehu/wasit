use std::{
    fs,
    io::stderr,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use color_eyre::eyre::{self, Context as _};
use tracing_bunyan_formatter::BunyanFormattingLayer;
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt as _;
use wazzi_fuzzer::{store::FuzzStore, Fuzzer, Runtime};
use wazzi_runners::{Node, Wamr, Wasmedge, Wasmer, Wasmtime};
use wazzi_wasi::seed::Seed;

#[derive(Parser, Debug, Clone)]
struct Cmd {
    #[arg(short, long)]
    initial_data: Option<PathBuf>,
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::Registry::default()
            .with(ErrorLayer::default())
            .with(BunyanFormattingLayer::new("wazzi".to_owned(), stderr)),
    )
    .wrap_err("failed to configure tracing")?;

    let cmd = Cmd::parse();
    let initial_data = cmd
        .initial_data
        .map(|path| fs::read(path))
        .transpose()
        .wrap_err("failed to read initial data file")?;
    let seed: Seed = serde_json::from_reader(
        fs::OpenOptions::new()
            .read(true)
            .open(PathBuf::from("seeds").join("00-creat.json"))
            .unwrap(),
    )
    .unwrap();
    let runtimes_dir = PathBuf::from("runtimes");
    let results_root = PathBuf::from("abc");

    fs::create_dir(&results_root)?;

    let results_root = results_root.canonicalize()?;
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
    let store = FuzzStore::new(&results_root).wrap_err("failed to initialize fuzz store")?;

    fuzzer
        .fuzz_loop(&store, initial_data.as_ref().map(|bytes| bytes.as_slice()))
        .wrap_err("fuzz loop error")?;

    Ok(())
}
