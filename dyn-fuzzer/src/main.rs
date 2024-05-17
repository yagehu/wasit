use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use eyre::Context as _;
use wazzi_dyn_fuzzer::Fuzzer;
use wazzi_dyn_spec::Environment;
use wazzi_runners::{Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime, Wazero};
use wazzi_store::FuzzStore;

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let fuzz_store = FuzzStore::new(Path::new("abc")).wrap_err("failed to init fuzz store")?;
    let mut fuzzer = Fuzzer::new(
        || -> Environment {
            let env = Environment::new();

            env
        },
        HashMap::from([
            (
                "node".to_owned(),
                Box::new(Node::new(Path::new("node"))) as Box<dyn WasiRunner>,
            ),
            ("wamr".to_owned(), Box::new(Wamr::new(Path::new("iwasm")))),
            (
                "wasmedge".to_owned(),
                Box::new(Wasmedge::new(Path::new("wasmedge"))),
            ),
            (
                "wasmer".to_owned(),
                Box::new(Wasmer::new(Path::new("wasmer"))),
            ),
            (
                "wasmtime".to_owned(),
                Box::new(Wasmtime::new(Path::new("wasmtime"))),
            ),
            (
                "wazero".to_owned(),
                Box::new(Wazero::new(Path::new("wazero"))),
            ),
        ]),
        PathBuf::new()
            .join("target")
            .join("debug")
            .join("wazzi-executor-pb.wasm"),
        fuzz_store,
    );

    fuzzer.fuzz();

    Ok(())
}
