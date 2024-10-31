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
use wazzi::{Call, ResourceIdx};

fn main() -> Result<(), eyre::Error> {
    color_eyre::install()?;

    let cmd = Command::parse();
    let entries = fs::read_dir(&cmd.dir)?
        .collect::<Result<Vec<_>, _>>()
        .wrap_err("failed to read directory entries")?;
    let entries = entries
        .into_iter()
        .map(|entry| {
            (
                entry.path(),
                entry
                    .file_name()
                    .into_string()
                    .unwrap()
                    .parse::<usize>()
                    .unwrap(),
            )
        })
        .sorted_by(|(_, idx_0), (_, idx_1)| Ord::cmp(idx_0, idx_1))
        .collect_vec();
    let mut runs = 0;
    let mut total_num_calls = 0;
    let mut max_trace_len = 0;
    let mut runtimes = None;
    let mut total_max_calls = 0;
    let mut total_max_resource_depth = 0;

    for (path, _run_idx) in entries {
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
                    entry
                        .file_name()
                        .into_string()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap(),
                )
            })
            .sorted_by(|(_, idx_0), (_, idx_1)| Ord::cmp(idx_0, idx_1))
            .collect_vec();
        let mut trace_len = 0;
        let mut graph = DiGraph::new();
        let mut resource_node_map = HashMap::new();
        let mut init_resources = HashSet::new();

        println!("Analyzing run {runs}.");

        for (path, idx) in call_entries {
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
                        let resource_node_idx =
                            graph.add_node(Node::Resource { idx: resource_idx });

                        resource_node_map.insert(resource_idx, resource_node_idx);
                        graph.add_edge(call_node_idx, resource_node_idx, Edge::Result);
                    }
                }
            }
        }

        let mut most_calls = 0;
        let mut max_resource_depth = 0;

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
                for child_resource_node_idx in
                    graph.neighbors_directed(func_node_idx, Direction::Outgoing)
                {
                    let child_resource = graph.node_weight(child_resource_node_idx).unwrap();
                    let child_resource_idx = match child_resource {
                        | Node::Resource { idx } => *idx,
                        | Node::Call { .. } => unreachable!(),
                    };

                    max_depth =
                        max_depth.max(tree_depth(graph, resource_node_map, child_resource_idx));
                }
            }

            max_depth + 1
        }

        for &init_resource in &init_resources {
            max_resource_depth =
                max_resource_depth.max(tree_depth(&graph, &resource_node_map, init_resource));
        }

        for (resource_idx, node_idx) in resource_node_map {
            if init_resources.contains(&resource_idx) {
                continue;
            }

            let calls = graph
                .neighbors_directed(node_idx, Direction::Outgoing)
                .count();

            most_calls = most_calls.max(calls);
        }

        fs::write(
            runtimes_dir.join(runtime).join("trace.dot"),
            format!("{:?}", Dot::with_config(&graph, &[])),
        )
        .unwrap();
        max_trace_len = max_trace_len.max(trace_len);
        total_max_calls += most_calls;
        total_max_resource_depth += max_resource_depth;
        runs += 1;
    }

    println!("# runs: {runs}");
    println!("max trace len: {max_trace_len}");
    println!(
        "average trace len: {:.2}",
        total_num_calls as f64 / runs as f64
    );
    println!(
        "average max calls involving one resource: {:2}",
        total_max_calls as f64 / runs as f64
    );
    println!(
        "average max resource depth {:.2}",
        total_max_resource_depth as f64 / runs as f64
    );

    Ok(())
}

#[derive(clap::Parser, Debug)]
struct Command {
    #[arg()]
    dir: PathBuf,
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
