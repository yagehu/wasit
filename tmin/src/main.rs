use std::{
    collections::{HashMap, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use color_eyre::eyre::{self, Context, ContextCompat};
use petgraph::{graph::NodeIndex, stable_graph::StableDiGraph};
use tracing_error::ErrorLayer;
use tracing_subscriber::layer::SubscriberExt as _;
use wazzi_fuzzer::{Fuzzer, Runtime};
use wazzi_runners::{Node, Wamr, Wasmedge, Wasmer, Wasmtime};
use wazzi_spec::{
    package::{StateEffect, Typeidx, TypeidxBorrow, Valtype},
    parsers::Span,
};
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

    #[arg()]
    action_idx: usize,
}

/// Communication relation.
#[derive(Clone, Debug)]
enum Com {
    /// Reads-from. W -> R
    Rf,

    /// Coherence-order. W -> W
    Co,
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

    let mut graph = StableDiGraph::new();
    let mut call_node_map: HashMap<usize, NodeIndex> = HashMap::new();

    for (i, call) in calls.iter().enumerate() {
        let node_idx = graph.add_node(call.clone());

        call_node_map.insert(i, node_idx);
    }

    let spec_str = fs::read_to_string("spec/preview1.witx").wrap_err("failed to read spec file")?;
    let spec = wazzi_spec::parsers::wazzi_preview1::Document::parse(Span::new(&spec_str))
        .unwrap()
        .into_package()
        .wrap_err("failed to process spec")?;
    let interface = spec
        .interface(TypeidxBorrow::Symbolic("wasi_snapshot_preview1"))
        .unwrap();

    for (i, call) in calls.iter().enumerate() {
        let func_spec = interface
            .function(&call.func)
            .wrap_err("func not in spec interface")?;
        let result_valtypes = func_spec.unpack_expected_result();
        let mut reads = HashSet::new();
        let mut writes = HashSet::new();
        let node_idx = *call_node_map.get(&i).unwrap();

        for (j, param_spec) in func_spec.params.iter().enumerate() {
            let param = call.params.get(j).unwrap();

            if let (Valtype::Typeidx(Typeidx::Symbolic(name)), Some(resource)) =
                (&param_spec.valtype, &param.resource)
            {
                match param_spec.state_effect {
                    | StateEffect::Read => reads.insert(resource.id),
                    | StateEffect::Write => writes.insert(resource.id),
                };
            }
        }

        for (j, result_valtype) in result_valtypes.iter().enumerate() {
            let result = call.results.get(j).unwrap();

            if let (Valtype::Typeidx(Typeidx::Symbolic(name)), Some(resource)) =
                (&result_valtype, &result.resource)
            {
                writes.insert(resource.id);
            }
        }

        for j in (i + 1)..calls.len() {
            let call2 = calls.get(j).unwrap();
            let func_spec2 = interface
                .function(&call2.func)
                .wrap_err("func not in spec interface")?;
            let node_idx2 = *call_node_map.get(&j).unwrap();

            for (k, param_spec2) in func_spec2.params.iter().enumerate() {
                let param2 = call2.params.get(k).unwrap();

                if let (Valtype::Typeidx(_), Some(resource)) =
                    (&param_spec2.valtype, &param2.resource)
                {
                    if writes.contains(&resource.id) {
                        match param_spec2.state_effect {
                            | StateEffect::Read => {
                                graph.add_edge(node_idx, node_idx2, Com::Rf);
                            },
                            | StateEffect::Write => {
                                graph.add_edge(node_idx, node_idx2, Com::Co);
                            },
                        }
                    }
                }
            }
        }
    }

    let action_idx = cmd.action_idx;
    let last_call_node_idx = *call_node_map.get(&action_idx).unwrap();
    let mut min_calls = Vec::new();
    let mut to_remove = HashSet::new();

    for i in 0..action_idx {
        let call_node_idx = *call_node_map.get(&i).unwrap();

        if petgraph::algo::has_path_connecting(&graph, call_node_idx, last_call_node_idx, None) {
            min_calls.push(calls[i].clone());
        } else {
            to_remove.insert(call_node_idx);
        }
    }

    for i in (action_idx + 1)..calls.len() {
        let call_node_idx = *call_node_map.get(&i).unwrap();

        to_remove.insert(call_node_idx);
    }

    min_calls.push(calls.last().unwrap().clone());

    for node_idx in to_remove {
        graph.remove_node(node_idx).unwrap();
    }

    let dot = format!(
        "{:?}",
        petgraph::dot::Dot::with_config(&graph, &[petgraph::dot::Config::NodeIndexLabel])
    );

    fs::write("dot", dot).unwrap();

    Ok(())
}
