use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    path::PathBuf,
};

use clap::Parser;
use color_eyre::eyre;
use eyre::Context;
use itertools::Itertools;
use petgraph::{
    dot::Dot,
    graph::{DiGraph, NodeIndex},
    Direction,
};
use serde::Serialize;
use wazzi::{Call, ResourceIdx};

#[derive(Serialize, PartialEq, Eq, Clone, Debug)]
struct Analysis {
    runs: Vec<RunMetadata>,
}

#[derive(Serialize, PartialEq, Eq, Clone, Debug)]
struct RunMetadata {
    id:     usize,
    ncalls: usize,
}

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let mut runs = Vec::new();
    let cmd = Command::parse();
    let entries = fs::read_dir(&cmd.dir)?
        .collect::<Result<Vec<_>, _>>()
        .wrap_err("failed to read directory entries")?;

    fs::create_dir(&cmd.out_dir)?;

    let out_dir = cmd.out_dir.canonicalize()?;
    let entries = entries
        .into_iter()
        .map(|entry| {
            (
                entry.path(),
                entry.file_name().into_string().unwrap().parse::<usize>().unwrap(),
            )
        })
        .sorted_by(|(_, idx_0), (_, idx_1)| Ord::cmp(idx_0, idx_1))
        .collect_vec();
    let mut nruns = 0;
    let mut total_num_calls = 0;
    let mut max_trace_len = 0;
    let mut max_trace_len_idx = 0;
    let mut runtimes = None;
    let mut total_max_calls = 0;
    let mut max_resource_depth = 0;
    let mut total_max_resource_depth = 0;

    for (path, run_idx) in entries {
        let runtimes_dir = path.join("runtimes");

        if runtimes.is_none() {
            runtimes = Some(
                fs::read_dir(&runtimes_dir)
                    .wrap_err("failed to read runtimes dir")?
                    .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                    .sorted()
                    .collect_vec(),
            );

            println!(
                "Found {} runtimes: {:?}",
                runtimes.as_ref().unwrap().len(),
                runtimes.as_ref().unwrap()
            );
        }

        let runtimes = runtimes.as_ref().unwrap();
        let runtime = runtimes.first().unwrap();
        let trace_dir = runtimes_dir.join(runtime).join("trace");
        let call_entries = fs::read_dir(&trace_dir)
            .unwrap()
            .into_iter()
            .map(|entry| entry.unwrap())
            .map(|entry| {
                (
                    entry.path(),
                    entry.file_name().into_string().unwrap().parse::<usize>().unwrap(),
                )
            })
            .sorted_by(|(_, idx_0), (_, idx_1)| Ord::cmp(idx_0, idx_1))
            .collect_vec();
        let mut trace_len = 0;
        let mut graph = DiGraph::new();
        let mut resource_node_map = HashMap::new();
        let mut init_resources = HashSet::new();
        let ncalls = call_entries.len();

        println!("Analyzing run {nruns}.");

        for (path, idx) in call_entries {
            if !path.join("call.json").exists() {
                break;
            }

            trace_len += 1;
            total_num_calls += 1;

            let call_file = File::open(path.join("call.json")).unwrap();
            let call: Call = serde_json::from_reader(&call_file).unwrap();
            let call_node_idx = graph.add_node(Node::Call {
                idx,
                name: call.function,
            });

            for param in call.params {
                if let Some(resource_idx) = param.resource_idx {
                    let resource_node_idx = match resource_node_map.get(&resource_idx) {
                        | Some(idx) => *idx,
                        | None => {
                            let idx = graph.add_node(Node::Resource { idx: resource_idx });

                            resource_node_map.insert(resource_idx, idx);
                            init_resources.insert(resource_idx);

                            idx
                        },
                    };

                    graph.add_edge(resource_node_idx, call_node_idx, Edge::Param);
                }
            }

            if let Some(results) = call.results {
                for result in results {
                    if let Some(resource_idx) = result.resource_idx {
                        let resource_node_idx = graph.add_node(Node::Resource { idx: resource_idx });

                        resource_node_map.insert(resource_idx, resource_node_idx);
                        graph.add_edge(call_node_idx, resource_node_idx, Edge::Result);
                    }
                }
            }
        }

        runs.push(RunMetadata { id: run_idx, ncalls });

        let mut most_calls = 0;
        let mut max_resource_depth_ = 0;

        fn tree_depth(
            graph: &DiGraph<Node, Edge>,
            resource_node_map: &HashMap<ResourceIdx, NodeIndex>,
            resource_idx: ResourceIdx,
        ) -> usize {
            let resource_node_idx = *resource_node_map.get(&resource_idx).unwrap();
            let funcs = graph
                .neighbors_directed(resource_node_idx, Direction::Outgoing)
                .collect_vec();

            if funcs.is_empty() {
                return 1;
            }

            let mut max_depth = 0;

            for func_node_idx in funcs {
                for child_resource_node_idx in graph.neighbors_directed(func_node_idx, Direction::Outgoing) {
                    let child_resource = graph.node_weight(child_resource_node_idx).unwrap();
                    let child_resource_idx = match child_resource {
                        | Node::Resource { idx } => *idx,
                        | Node::Call { .. } => unreachable!(),
                    };

                    max_depth = max_depth.max(tree_depth(graph, resource_node_map, child_resource_idx));
                }
            }

            max_depth + 1
        }

        for &init_resource in &init_resources {
            max_resource_depth_ = max_resource_depth_.max(tree_depth(&graph, &resource_node_map, init_resource));
        }

        for (resource_idx, node_idx) in resource_node_map {
            if init_resources.contains(&resource_idx) {
                continue;
            }

            let calls = graph.neighbors_directed(node_idx, Direction::Outgoing).count();

            most_calls = most_calls.max(calls);
        }

        fs::write(
            runtimes_dir.join(runtime).join("trace.dot"),
            format!("{:?}", Dot::with_config(&graph, &[])),
        )
        .unwrap();

        if trace_len > max_trace_len {
            max_trace_len_idx = run_idx;
        }

        max_trace_len = max_trace_len.max(trace_len);
        total_max_calls += most_calls;
        max_resource_depth = max_resource_depth.max(max_resource_depth_);
        total_max_resource_depth += max_resource_depth_;
        nruns += 1;
    }

    let analysis = Analysis { runs };

    serde_json::to_writer(
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(out_dir.join("all.json"))?,
        &analysis,
    )?;

    let mut runs = csv::Writer::from_writer(
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(out_dir.join("runs.csv"))?,
    );

    for run in &analysis.runs {
        runs.serialize(&run)?;
    }

    println!("# runs: {nruns}");
    println!("max trace len: {max_trace_len}");
    println!("max trace len idx: {max_trace_len_idx}");
    println!("average trace len: {:.2}", total_num_calls as f64 / nruns as f64);
    println!(
        "average max calls involving one resource: {:2}",
        total_max_calls as f64 / nruns as f64
    );
    println!("max resource depth: {:.2}", max_resource_depth,);
    println!(
        "average max resource depth {:.2}",
        total_max_resource_depth as f64 / nruns as f64
    );

    Ok(())
}

#[derive(clap::Parser, Debug)]
struct Command {
    #[arg()]
    dir: PathBuf,

    #[arg()]
    out_dir: PathBuf,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Node {
    Resource { idx: ResourceIdx },
    Call { idx: usize, name: String },
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Edge {
    Param,
    Result,
}
