#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

use arbitrary::Unstructured;
use eyre::{eyre as err, Context};
use idxspace::IndexSpace;
use itertools::Itertools;
use petgraph::{data::DataMap as _, graph::DiGraph, visit::IntoNeighborsDirected};
use z3::ast::{forall_const, lambda_const, Ast, Bool, Dynamic, Int};

use super::CallStrategy;
use crate::{
    spec::{
        witx::slang::{self, Term},
        FlagsValue,
        Function,
        ListValue,
        PointerValue,
        RecordValue,
        Spec,
        TypeDef,
        VariantValue,
        WasiType,
        WasiValue,
    },
    Environment,
    ResourceIdx,
    RuntimeContext,
};

#[derive(Clone, Debug)]
struct State {
    preopens:  IndexSpace<ResourceIdx, PreopenFs>,
    fds_graph: DiGraph<ResourceIdx, String>,
    fds_idxs:  BTreeMap<ResourceIdx, petgraph::graph::NodeIndex>,
    resources: BTreeMap<ResourceIdx, WasiValue>,
}

impl State {
    fn new() -> Self {
        Self {
            preopens:  Default::default(),
            fds_graph: Default::default(),
            fds_idxs:  Default::default(),
            resources: Default::default(),
        }
    }

    fn push_preopen(&mut self, idx: ResourceIdx, path: &Path) {
        let preopen = PreopenFs::new(path).unwrap();

        self.preopens.push(idx, preopen);
    }

    fn push_resource(&mut self, idx: ResourceIdx, tdef: &TypeDef, value: WasiValue) {
        if tdef.name == "fd" {
            let node_idx = self.fds_graph.add_node(idx);
            let (_parent_member, parent_value) = tdef
                .state
                .as_ref()
                .unwrap()
                .record()
                .unwrap()
                .members
                .iter()
                .zip(value.record().unwrap().members.iter())
                .find(|(member, _val)| member.name == "parent")
                .unwrap();
            let (_path_member, path_value) = tdef
                .state
                .as_ref()
                .unwrap()
                .record()
                .unwrap()
                .members
                .iter()
                .zip(value.record().unwrap().members.iter())
                .find(|(member, _val)| member.name == "path")
                .unwrap();
            let path = String::from_utf8(path_value.string().unwrap().to_vec()).unwrap();

            self.fds_idxs.insert(idx, node_idx);

            if !path.is_empty() {
                let parent_resource_idx =
                    ResourceIdx(parent_value.u64().unwrap().try_into().unwrap());
                let parent_node_idx = *self.fds_idxs.get(&parent_resource_idx).unwrap();

                self.fds_graph.add_edge(node_idx, parent_node_idx, path);
            }
        }

        self.resources.insert(idx, value);
    }

    fn declare<'ctx>(
        &self,
        mut aop: ArbitraryOrPresolved<'_, '_>,
        spec: &Spec,
        ctx: &'ctx z3::Context,
        types: &'ctx StateTypes<'ctx>,
        env: &Environment,
        function: &Function,
        contract: Option<&Term>,
    ) -> StateDecls<'ctx> {
        let mut preopens = BTreeMap::default();
        let mut resources = BTreeMap::new();

        for (&fd_idx, preopen) in self.preopens.iter() {
            let root = preopen.root.declare(ctx, types);

            preopens.insert(fd_idx, PreopenFsEncoding { root });
        }

        for (&idx, _value) in self.resources.iter() {
            resources.insert(
                idx,
                Dynamic::fresh_const(
                    ctx,
                    &format!("{}--", env.resources_types.get(&idx).unwrap()),
                    &types
                        .resources
                        .get(env.resources_types.get(&idx).unwrap())
                        .unwrap()
                        .sort,
                ),
            );
        }

        let mut to_solves = ToSolves::default();

        if let Some(term) = &contract {
            fn scan_primed_in_output_contract<'ctx>(
                ctx: &'ctx z3::Context,
                types: &'ctx StateTypes<'ctx>,
                spec: &Spec,
                function: &Function,
                term: &Term,
                to_solves: &mut ToSolves<'ctx>,
            ) {
                match term {
                    | Term::Foldl(_t) => todo!(),
                    | Term::Lambda(_t) => todo!(),
                    | Term::Map(_t) => todo!(),
                    | Term::Binding(_) => todo!(),
                    | Term::True => todo!(),
                    | Term::String(_) => (),
                    | Term::Not(_t) => todo!(),
                    | Term::And(t) => {
                        for clause in &t.clauses {
                            scan_primed_in_output_contract(
                                ctx, types, spec, function, clause, to_solves,
                            );
                        }
                    },
                    | Term::Or(t) => {
                        for clause in &t.clauses {
                            scan_primed_in_output_contract(
                                ctx, types, spec, function, clause, to_solves,
                            );
                        }
                    },
                    | Term::RecordField(t) => scan_primed_in_output_contract(
                        ctx, types, spec, function, &t.target, to_solves,
                    ),
                    | Term::Param(_t) => (),
                    | Term::Result(t) => {
                        let function_result = function
                            .results
                            .iter()
                            .find(|result| result.name == t.name.strip_suffix('\'').unwrap())
                            .unwrap();
                        let tdef = function_result.tref.resolve(spec);
                        let datatype = types.resources.get(&tdef.name).unwrap();

                        to_solves.results.insert(
                            t.name.strip_suffix('\'').unwrap().to_string(),
                            Dynamic::new_const(ctx, format!("result--{}", t.name), &datatype.sort),
                        );
                    },
                    | Term::ResourceId(_) => (),
                    | Term::FlagsGet(_t) => todo!(),
                    | Term::ListLen(_t) => todo!(),
                    | Term::IntWrap(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.op, to_solves)
                    },
                    | Term::IntConst(_t) => (),
                    | Term::IntAdd(_t) => todo!(),
                    | Term::IntGt(_t) => todo!(),
                    | Term::IntLe(_t) => todo!(),
                    | Term::U64Const(t) => scan_primed_in_output_contract(
                        ctx, types, spec, function, &t.term, to_solves,
                    ),
                    | Term::StrAt(t) => {
                        scan_primed_in_output_contract(
                            ctx, types, spec, function, &t.lhs, to_solves,
                        );
                        scan_primed_in_output_contract(
                            ctx, types, spec, function, &t.rhs, to_solves,
                        );
                    },
                    | Term::ValueEq(t) => {
                        scan_primed_in_output_contract(
                            ctx, types, spec, function, &t.lhs, to_solves,
                        );
                        scan_primed_in_output_contract(
                            ctx, types, spec, function, &t.rhs, to_solves,
                        );
                    },
                    | Term::VariantConst(t) => {
                        if let Some(payload) = &t.payload {
                            scan_primed_in_output_contract(
                                ctx, types, spec, function, payload, to_solves,
                            );
                        }
                    },
                    | Term::FsFileTypeGet(_t) => (),
                    | Term::NoNonExistentDirBacktrack(_t) => todo!(),
                }
            }

            scan_primed_in_output_contract(ctx, types, spec, function, term, &mut to_solves);
        }

        let params = function
            .params
            .iter()
            .map(|param| (&param.name, param.tref.resolve(spec)))
            .map(|(param_name, tdef)| {
                let datatype = types.resources.get(&tdef.name).unwrap();

                match tdef.name.as_str() {
                    | "path" => {
                        let len = match &mut aop {
                            | ArbitraryOrPresolved::Arbitrary(u) => u.choose_index(16).unwrap(),
                            | ArbitraryOrPresolved::Presolved(lens) => {
                                *lens.get(param_name).unwrap()
                            },
                        };

                        (
                            param_name.to_owned(),
                            ParamDecl::Path {
                                segments: (0..len)
                                    .map(|_i| {
                                        Dynamic::fresh_const(
                                            ctx,
                                            "param-segment--",
                                            &types.segment.sort,
                                        )
                                    })
                                    .collect_vec(),
                            },
                        )
                    },
                    | _ => (
                        param_name.to_owned(),
                        ParamDecl::Node(Dynamic::fresh_const(ctx, "param--", &datatype.sort)),
                    ),
                }
            })
            .collect();

        StateDecls {
            fd_file: z3::FuncDecl::new(
                ctx,
                "fd-file",
                &[&types.resources.get("fd").unwrap().sort, &types.file.sort],
                &z3::Sort::bool(ctx),
            ),
            children: z3::FuncDecl::new(
                ctx,
                "children",
                &[&types.file.sort, &z3::Sort::string(ctx), &types.file.sort],
                &z3::Sort::bool(ctx),
            ),
            preopens,
            resources,
            params,
            to_solves,
        }
    }

    fn encode<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        env: &Environment,
        types: &'ctx StateTypes<'ctx>,
        decls: &'ctx StateDecls<'ctx>,
        spec: &Spec,
        function: &Function,
        params: Option<&[(WasiValue, Option<ResourceIdx>)]>,
        contract: Option<&Term>,
    ) -> Bool<'ctx> {
        let mut clauses = Vec::new();
        let fds_graph_rev = petgraph::visit::Reversed(&self.fds_graph);
        let mut topo = petgraph::visit::Topo::new(&fds_graph_rev);
        let mut dirs: BTreeMap<ResourceIdx, &DirectoryEncoding> = Default::default();
        let mut fd_file_pairs = Vec::new();

        for (&resource_idx, preopen) in decls.preopens.iter() {
            dirs.insert(resource_idx, &preopen.root);
        }

        for (&idx, preopen) in decls.preopens.iter() {
            fd_file_pairs.push((
                decls.resources.get(&idx).expect(&format!("{}", idx.0)),
                &preopen.root.node,
            ));
        }

        while let Some(node_idx) = topo.next(fds_graph_rev) {
            let fd_resource_idx = *fds_graph_rev.node_weight(node_idx).unwrap();
            let dir = match dirs.get(&fd_resource_idx) {
                | Some(&dir) => dir,
                | None => continue,
            };

            for child_node_idx in
                fds_graph_rev.neighbors_directed(node_idx, petgraph::Direction::Outgoing)
            {
                let mut curr = FileEncodingRef::Directory(dir);
                let mut prevs = Vec::new();
                let child_fd_resource_idx = *fds_graph_rev.node_weight(child_node_idx).unwrap();
                let edge_idx = self.fds_graph.find_edge(child_node_idx, node_idx).unwrap();
                let path = self.fds_graph.edge_weight(edge_idx).unwrap();

                for component in PathBuf::from(path).components() {
                    let component = component.as_os_str().to_str().unwrap();

                    match component {
                        | "." => (),
                        | ".." => curr = prevs.pop().unwrap(),
                        | component => {
                            let child = curr.directory().unwrap().children.get(component).unwrap();

                            prevs.push(curr);

                            curr = match child {
                                | FileEncoding::Directory(d) => FileEncodingRef::Directory(d),
                                | FileEncoding::RegularFile(f) => FileEncodingRef::RegularFile(f),
                            }
                        },
                    }
                }

                fd_file_pairs.push((
                    decls.resources.get(&child_fd_resource_idx).unwrap(),
                    match curr {
                        | FileEncodingRef::Directory(d) => &d.node,
                        | FileEncodingRef::RegularFile(f) => &f.node,
                    },
                ));

                if let FileEncodingRef::Directory(d) = curr {
                    dirs.insert(child_fd_resource_idx, d);
                }
            }
        }

        {
            let mut all_dirs = decls
                .preopens
                .values()
                .map(|preopen| &preopen.root.node)
                .collect_vec();
            let mut all_files = vec![];

            for (_idx, preopen) in decls.preopens.iter() {
                let mut stack = vec![&preopen.root];

                while let Some(dir) = stack.pop() {
                    for (_filename, child) in dir.children.iter() {
                        match child {
                            | FileEncoding::Directory(d) => {
                                all_dirs.push(&d.node);
                                stack.push(&d);
                            },
                            | FileEncoding::RegularFile(f) => all_files.push(&f.node),
                        }
                    }
                }
            }

            clauses.push(Bool::and(
                ctx,
                all_dirs
                    .iter()
                    .map(|&dir| {
                        types.file.variants[0]
                            .tester
                            .apply(&[dir])
                            .as_bool()
                            .unwrap()
                    })
                    .collect_vec()
                    .as_slice(),
            ));
            clauses.push(Bool::and(
                ctx,
                all_files
                    .into_iter()
                    .map(|dir| {
                        types.file.variants[1]
                            .tester
                            .apply(&[dir])
                            .as_bool()
                            .unwrap()
                    })
                    .collect_vec()
                    .as_slice(),
            ));

            for (i, &dir_a) in all_dirs.iter().enumerate() {
                for j in (i + 1)..all_dirs.len() {
                    let dir_b = *all_dirs.get(j).unwrap();

                    clauses.push(
                        types.file.variants[0].accessors[0]
                            .apply(&[dir_a])
                            ._eq(&types.file.variants[0].accessors[0].apply(&[dir_b]))
                            .not(),
                    );
                }
            }

            // Assign IDs to all files.

            let mut stack = decls.preopens.values().map(|p| &p.root).collect_vec();
            let mut idx = 0;

            for preopen in decls.preopens.values() {
                clauses.push(
                    types.file.variants[0].accessors[0]
                        .apply(&[&preopen.root.node])
                        .as_int()
                        .unwrap()
                        ._eq(&Int::from_u64(ctx, idx)),
                );
                idx += 1;
            }

            while let Some(dir) = stack.pop() {
                for (_name, child) in &dir.children {
                    match child {
                        | FileEncoding::Directory(d) => {
                            clauses.push(
                                types.file.variants[0].accessors[0]
                                    .apply(&[&d.node])
                                    .as_int()
                                    .unwrap()
                                    ._eq(&Int::from_u64(ctx, idx)),
                            );
                            stack.push(d);
                            idx += 1;
                        },
                        | FileEncoding::RegularFile(f) => {
                            clauses.push(
                                types.file.variants[1].accessors[0]
                                    .apply(&[&f.node])
                                    .as_int()
                                    .unwrap()
                                    ._eq(&Int::from_u64(ctx, idx)),
                            );
                            idx += 1;
                        },
                    }
                }
            }

            let file_a = Dynamic::fresh_const(ctx, "", &types.file.sort);
            let file_b = Dynamic::fresh_const(ctx, "", &types.file.sort);

            clauses.push(forall_const(
                ctx,
                &[&file_a, &file_b],
                &[],
                &Bool::and(
                    ctx,
                    &[
                        file_a._eq(&file_b).not(),
                        types.file.variants[0]
                            .tester
                            .apply(&[&file_a])
                            .as_bool()
                            .unwrap(),
                        types.file.variants[0]
                            .tester
                            .apply(&[&file_b])
                            .as_bool()
                            .unwrap(),
                    ],
                )
                .implies(
                    &types.file.variants[0].accessors[0]
                        .apply(&[&file_a])
                        ._eq(&types.file.variants[0].accessors[0].apply(&[&file_b]))
                        .not(),
                ),
            ));
            clauses.push(forall_const(
                ctx,
                &[&file_a, &file_b],
                &[],
                &Bool::and(
                    ctx,
                    &[
                        file_a._eq(&file_b).not(),
                        types.file.variants[1]
                            .tester
                            .apply(&[&file_a])
                            .as_bool()
                            .unwrap(),
                        types.file.variants[1]
                            .tester
                            .apply(&[&file_b])
                            .as_bool()
                            .unwrap(),
                    ],
                )
                .implies(
                    &types.file.variants[1].accessors[0]
                        .apply(&[&file_a])
                        ._eq(&types.file.variants[1].accessors[0].apply(&[&file_b]))
                        .not(),
                ),
            ));
        }

        // fd -> file mapping
        {
            let some_fd = Dynamic::fresh_const(ctx, "", &types.resources.get("fd").unwrap().sort);
            let some_file = Dynamic::fresh_const(ctx, "", &types.file.sort);

            clauses.push(forall_const(
                ctx,
                &[&some_fd, &some_file],
                &[],
                &Bool::or(
                    ctx,
                    fd_file_pairs
                        .into_iter()
                        .map(|(fd, file)| Bool::and(ctx, &[fd._eq(&some_fd), file._eq(&some_file)]))
                        .collect_vec()
                        .as_slice(),
                )
                .ite(
                    &decls
                        .fd_file
                        .apply(&[&some_fd, &some_file])
                        .as_bool()
                        .unwrap(),
                    &decls
                        .fd_file
                        .apply(&[&some_fd, &some_file])
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
            ));
        }

        // Constrain all the resource values.
        {
            clauses.push(Bool::and(
                ctx,
                decls
                    .params
                    .iter()
                    .filter_map(|(param_name, param_node)| {
                        let param = function
                            .params
                            .iter()
                            .find(|p| &p.name == param_name)
                            .unwrap();
                        let tdef = param.tref.resolve(spec);

                        if tdef.state.is_some() {
                            let idxs = env
                                .resources_by_types
                                .get(&tdef.name)
                                .cloned()
                                .unwrap_or_default();

                            Some(Bool::or(
                                ctx,
                                idxs.iter()
                                    .map(|&idx| {
                                        let resource = env.resources.get(idx).unwrap();

                                        types.encode_wasi_value(
                                            ctx,
                                            spec,
                                            param_node,
                                            tdef,
                                            &resource.state,
                                        )
                                    })
                                    .collect_vec()
                                    .as_slice(),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect_vec()
                    .as_slice(),
            ));
        }

        // Children relation. Maps any file (directory) to its children.
        {
            let mut children = Vec::new();

            for (&_idx, preopen) in &decls.preopens {
                let mut stack = vec![&preopen.root];

                while let Some(dir) = stack.pop() {
                    for (name, child) in &dir.children {
                        children.push((&dir.node, name, child.node()));

                        match child {
                            | FileEncoding::Directory(d) => stack.push(d),
                            | FileEncoding::RegularFile(_f) => continue,
                        }
                    }
                }
            }

            let some_dir = Dynamic::fresh_const(ctx, "", &types.file.sort);
            let some_file = Dynamic::fresh_const(ctx, "", &types.file.sort);
            let some_name = z3::ast::String::fresh_const(ctx, "");

            clauses.push(forall_const(
                ctx,
                &[&some_dir, &some_file, &some_name],
                &[],
                &Bool::or(
                    ctx,
                    children
                        .into_iter()
                        .map(|(dir, name, file)| {
                            Bool::and(
                                ctx,
                                &[
                                    dir._eq(&some_dir),
                                    z3::ast::String::from_str(ctx, name.as_str())
                                        .unwrap()
                                        ._eq(&some_name),
                                    file._eq(&some_file),
                                ],
                            )
                        })
                        .collect_vec()
                        .as_slice(),
                )
                .ite(
                    &decls
                        .children
                        .apply(&[&some_dir, &some_name, &some_file])
                        .as_bool()
                        .unwrap(),
                    &decls
                        .children
                        .apply(&[&some_dir, &some_name, &some_file])
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
            ));
        }

        let separator = types.segment.variants.first().unwrap();
        let component = types.segment.variants.get(1).unwrap();
        let paths = decls
            .params
            .iter()
            .filter_map(|(_param_name, decl)| match decl {
                | ParamDecl::Node(_) => None,
                | ParamDecl::Path { segments } => Some(segments),
            })
            .collect_vec();

        // Segment components cannot be empty.
        for &path in paths.iter() {
            clauses.push(Bool::and(
                &ctx,
                path.iter()
                    .map(|segment| {
                        component
                            .tester
                            .apply(&[segment])
                            .as_bool()
                            .unwrap()
                            .implies(&Bool::and(
                                ctx,
                                &[
                                    &component.accessors[0]
                                        .apply(&[segment])
                                        .as_string()
                                        .unwrap()
                                        .length()
                                        .gt(&Int::from_u64(ctx, 0)),
                                    &Bool::or(
                                        ctx,
                                        &[
                                            &component.accessors[0]
                                                .apply(&[segment])
                                                .as_string()
                                                .unwrap()
                                                ._eq(&z3::ast::String::from_str(ctx, "a").unwrap()),
                                            &component.accessors[0]
                                                .apply(&[segment])
                                                .as_string()
                                                .unwrap()
                                                ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                                            &component.accessors[0]
                                                .apply(&[segment])
                                                .as_string()
                                                .unwrap()
                                                ._eq(
                                                    &z3::ast::String::from_str(ctx, "..").unwrap(),
                                                ),
                                        ],
                                    ),
                                ],
                            ))
                    })
                    .collect_vec()
                    .as_slice(),
            ));
        }

        // Adjacent segments can't both be components or separators.
        for path in paths.iter() {
            for (i, segment) in path.iter().enumerate().skip(1) {
                let prev = path.get(i - 1).unwrap();

                clauses.push(Bool::and(
                    ctx,
                    &[
                        separator
                            .tester
                            .apply(&[segment])
                            .as_bool()
                            .unwrap()
                            .implies(&separator.tester.apply(&[prev]).as_bool().unwrap().not()),
                        component
                            .tester
                            .apply(&[segment])
                            .as_bool()
                            .unwrap()
                            .implies(&component.tester.apply(&[prev]).as_bool().unwrap().not()),
                    ],
                ));
            }
        }

        // The first segment must be a component.
        // {
        //     let some_path = Dynamic::fresh_const(ctx, "", &path_datatype.sort);
        //     let segments = segments_accessor.apply(&[&some_path]).as_seq().unwrap();

        //     clauses.push(forall_const(
        //         ctx,
        //         &[&some_path],
        //         &[&z3::Pattern::new(ctx, &[&segments])],
        //         &segments.length().gt(&Int::from_u64(ctx, 0)).implies(
        //             &types.segment.variants[1]
        //                 .tester
        //                 .apply(&[&segments.nth(&Int::from_u64(ctx, 0))])
        //                 .as_bool()
        //                 .unwrap(),
        //         ),
        //     ));
        // }

        // Components cannot contain slash "/".
        // {
        //     let some_path = Dynamic::fresh_const(ctx, "", &path_datatype.sort);
        //     let some_idx = Int::fresh_const(ctx, "");

        //     clauses.push(forall_const(
        //         ctx,
        //         &[&some_idx, &some_path],
        //         &[],
        //         &Bool::and(
        //             ctx,
        //             &[
        //                 // The index is in the range [0, segments.len())
        //                 Int::from_u64(ctx, 0).le(&some_idx),
        //                 some_idx.lt(&segments_accessor
        //                     .apply(&[&some_path])
        //                     .as_seq()
        //                     .unwrap()
        //                     .length()),
        //                 // Segment[i] is a component.
        //                 types.segment.variants[1]
        //                     .tester
        //                     .apply(&[&segments_accessor
        //                         .apply(&[&some_path])
        //                         .as_seq()
        //                         .unwrap()
        //                         .nth(&some_idx)])
        //                     .as_bool()
        //                     .unwrap(),
        //             ],
        //         )
        //         .implies(
        //             // segment[i] cannot contain `/`,
        //             &types.segment.variants[1].accessors[0]
        //                 .apply(&[&segments_accessor
        //                     .apply(&[&some_path])
        //                     .as_seq()
        //                     .unwrap()
        //                     .nth(&some_idx)])
        //                 .as_string()
        //                 .unwrap()
        //                 .contains(&z3::ast::String::from_str(ctx, "/").unwrap())
        //                 .not(),
        //         ),
        //     ));
        // }

        // Constrain param resources.
        for (param_name, param_node) in decls.params.iter() {
            let param = function
                .params
                .iter()
                .find(|param| &param.name == param_name)
                .unwrap();
            let param_tdef = param.tref.resolve(spec);

            if param_tdef.wasi == WasiType::String && params.is_none() {
                // let datatype = types.resources.get(&param_tdef.name).unwrap();
                // let len = u.choose_index(64).unwrap() as u64 + 1;

                // TODO(yage)
                // Special case: Sequence solving is slow. Impose an exact length.
                // clauses.push(
                //     datatype.variants[0].accessors[0]
                //         .apply(&[param_node])
                //         .as_seq()
                //         .unwrap()
                //         .length()
                //         ._eq(&Int::from_u64(ctx, len)),
                // );

                // TODO(yage)
                // Special case: constraint characters in path string.
                // {
                //     let some_idx = Int::fresh_const(ctx, "");

                //     clauses.push(forall_const(
                //         ctx,
                //         &[&some_idx],
                //         &[],
                //         &Bool::and(
                //             ctx,
                //             &[
                //                 &Int::from_u64(ctx, 0).le(&some_idx),
                //                 &some_idx.lt(&Int::from_u64(ctx, len)),
                //             ],
                //         )
                //         .implies(&Bool::or(
                //             ctx,
                //             &[
                //                 &datatype.variants[0].accessors[0]
                //                     .apply(&[param_node])
                //                     .as_string()
                //                     .unwrap()
                //                     .at(&some_idx)
                //                     ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                //                 &datatype.variants[0].accessors[0]
                //                     .apply(&[param_node])
                //                     .as_string()
                //                     .unwrap()
                //                     .at(&some_idx)
                //                     ._eq(&z3::ast::String::from_str(ctx, "a").unwrap()),
                //                 &datatype.variants[0].accessors[0]
                //                     .apply(&[param_node])
                //                     .as_string()
                //                     .unwrap()
                //                     .at(&some_idx)
                //                     ._eq(&z3::ast::String::from_str(ctx, "/").unwrap()),
                //             ],
                //         )),
                //     ));
                // }
            }

            if param_tdef.state.is_none() {
                continue;
            }

            let empty = BTreeSet::new();
            let resource_idxs = match env.resources_by_types.get(&param_tdef.name) {
                | Some(idxs) => idxs,
                | None => &empty,
            };

            clauses.push(Bool::or(
                ctx,
                resource_idxs
                    .iter()
                    .map(|&idx| {
                        let resource_node = decls.resources.get(&idx).unwrap();

                        param_node.node()._eq(resource_node)
                    })
                    .collect_vec()
                    .as_slice(),
            ));
        }

        if let Some(term) = contract {
            let empty_eval_ctx = BTreeMap::new();

            clauses.push(
                self.term_to_z3_ast(
                    ctx,
                    env,
                    &empty_eval_ctx,
                    spec,
                    types,
                    decls,
                    term,
                    function,
                    params,
                )
                .0
                .as_bool()
                .unwrap(),
            );
        }

        Bool::and(ctx, &clauses)
    }

    fn term_to_z3_ast<'ctx, 'spec>(
        &self,
        ctx: &'ctx z3::Context,
        env: &Environment,
        eval_ctx: &BTreeMap<String, (Dynamic<'ctx>, Type)>,
        spec: &'spec Spec,
        types: &'ctx StateTypes<'ctx>,
        decls: &'ctx StateDecls<'ctx>,
        term: &Term,
        function: &Function,
        params: Option<&[(WasiValue, Option<ResourceIdx>)]>,
    ) -> (Dynamic<'ctx>, Type) {
        match term {
            | Term::Foldl(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.target, function, params,
                );
                let (acc, acc_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.acc, function, params,
                );
                let (func, _func_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.func, function, params,
                );

                (
                    types
                        .resources
                        .get(&target_type.wasi().unwrap().name)
                        .unwrap()
                        .variants[0]
                        .accessors[0]
                        .apply(&[&target])
                        .as_seq()
                        .unwrap()
                        .foldl(&func.as_array().unwrap(), &acc),
                    acc_type,
                )
            },
            | Term::Lambda(t) => {
                let bounds_ = t
                    .bounds
                    .iter()
                    .map(|bound| {
                        let (sort, t) = match &bound.tref {
                            | slang::TypeRef::Wasi(wasi) => (
                                types.resources.get(wasi).unwrap().sort.clone(),
                                Type::Wasi(spec.types.get_by_key(wasi).unwrap().clone()),
                            ),
                            | slang::TypeRef::Wazzi(wazzi_type) => match wazzi_type {
                                | slang::WazziType::Bool => {
                                    (z3::Sort::bool(ctx), Type::Wazzi(WazziType::Bool))
                                },
                                | slang::WazziType::Int => {
                                    (z3::Sort::int(ctx), Type::Wazzi(WazziType::Int))
                                },
                            },
                        };

                        (bound.name.clone(), Dynamic::fresh_const(ctx, "", &sort), t)
                    })
                    .collect_vec();
                let bounds = bounds_.iter().map(|b| &b.1 as &dyn Ast).collect_vec();
                let eval_ctx = bounds_
                    .iter()
                    .map(|b| (b.0.to_owned(), (b.1.clone(), b.2.clone())))
                    .collect();
                let (body, r#type) = self.term_to_z3_ast(
                    ctx, env, &eval_ctx, spec, types, decls, &t.body, function, params,
                );

                (
                    Dynamic::from_ast(&lambda_const(ctx, bounds.as_slice(), &body)),
                    Type::Wazzi(WazziType::Lambda(Box::new(LambdaType { range: r#type }))),
                )
            },
            | Term::Map(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.target, function, params,
                );
                let (func, func_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.func, function, params,
                );
                let range = match &func_type {
                    | Type::Wasi(_type_def) => panic!(),
                    | Type::Wazzi(wazzi_type) => match wazzi_type {
                        | WazziType::Int => todo!(),
                        | WazziType::Bool => todo!(),
                        | WazziType::Lambda(lambda) => lambda.range.clone(),
                        | WazziType::List(_) => todo!(),
                        | WazziType::String => todo!(),
                    },
                };

                (
                    types
                        .resources
                        .get(&target_type.wasi().unwrap().name)
                        .unwrap()
                        .variants[0]
                        .accessors[0]
                        .apply(&[&target])
                        .as_seq()
                        .unwrap()
                        .map(&func.as_array().unwrap()),
                    Type::Wazzi(WazziType::List(Box::new(range))),
                )
            },
            | Term::Binding(name) => eval_ctx.get(name).unwrap().clone(),
            | Term::True => (
                Dynamic::from_ast(&Bool::from_bool(ctx, true)),
                Type::Wazzi(WazziType::Bool),
            ),
            | Term::String(s) => (
                Dynamic::from_ast(&z3::ast::String::from_str(ctx, s).unwrap()),
                Type::Wazzi(WazziType::String),
            ),
            | Term::Not(t) => {
                let (term, r#type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.term, function, params,
                );

                (Dynamic::from_ast(&term.as_bool().unwrap().not()), r#type)
            },
            | Term::And(t) => (
                Dynamic::from_ast(&Bool::and(
                    ctx,
                    t.clauses
                        .iter()
                        .map(|clause| {
                            self.term_to_z3_ast(
                                ctx, env, eval_ctx, spec, types, decls, clause, function, params,
                            )
                            .0
                            .simplify()
                            .as_bool()
                            .unwrap()
                        })
                        .collect_vec()
                        .as_slice(),
                )),
                Type::Wazzi(WazziType::Bool),
            ),
            | Term::Or(t) => (
                Dynamic::from_ast(&Bool::or(
                    ctx,
                    t.clauses
                        .iter()
                        .map(|clause| {
                            self.term_to_z3_ast(
                                ctx, env, eval_ctx, spec, types, decls, clause, function, params,
                            )
                            .0
                            .as_bool()
                            .unwrap()
                        })
                        .collect_vec()
                        .as_slice(),
                )),
                Type::Wazzi(WazziType::Bool),
            ),
            | Term::RecordField(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.target, function, params,
                );
                let target_tdef = target_type.wasi().unwrap();
                let target_datatype = types.resources.get(&target_tdef.name).unwrap();
                let wasi_type = match &target_tdef.state {
                    | Some(state) => state,
                    | None => &target_tdef.wasi,
                };
                let record_type = wasi_type.record().unwrap();
                let (i, member) = record_type
                    .members
                    .iter()
                    .enumerate()
                    .find(|(_i, member)| member.name == t.member)
                    .unwrap();

                (
                    target_datatype.variants[0].accessors[i].apply(&[&target]),
                    Type::Wasi(member.tref.resolve(spec).clone()),
                )
            },
            | Term::Param(t) => {
                let tdef = function
                    .params
                    .iter()
                    .find(|p| p.name == t.name)
                    .unwrap()
                    .tref
                    .resolve(spec);
                let param = decls.params.get(&t.name).expect(&format!("{}", t.name));

                (param.node().to_owned(), Type::Wasi(tdef.to_owned()))
            },
            | Term::Result(t) => {
                if t.name.ends_with('\'') {
                    let function_result = function
                        .results
                        .iter()
                        .find(|result| result.name == t.name.strip_suffix('\'').unwrap())
                        .unwrap();
                    let tdef = function_result.tref.resolve(spec);

                    (
                        decls
                            .to_solves
                            .results
                            .get(t.name.strip_suffix('\'').unwrap())
                            .unwrap()
                            .clone(),
                        Type::Wasi(tdef.to_owned()),
                    )
                } else {
                    todo!()
                }
            },
            | Term::ResourceId(name) => {
                let (param_idx, _function_param) = function
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, param)| &param.name == name)
                    .unwrap();
                let resource_idx = params.unwrap().get(param_idx).unwrap().1.unwrap();

                (
                    Dynamic::from_ast(&Int::from_u64(ctx, resource_idx.0 as u64)),
                    Type::Wazzi(WazziType::Int),
                )
            },
            | Term::FlagsGet(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.target, function, params,
                );
                let target_tdef = target_type.wasi().unwrap();
                let target_datatype = types.resources.get(&target_tdef.name).unwrap();
                let wasi_type = match &target_tdef.state {
                    | Some(state) => state,
                    | None => &target_tdef.wasi,
                };
                let flags_type = wasi_type.flags().unwrap();
                let (i, _name) = flags_type
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_i, name)| *name == &t.field)
                    .unwrap();

                (
                    target_datatype.variants[0].accessors[i].apply(&[&target]),
                    Type::Wazzi(WazziType::Bool),
                )
            },
            | Term::IntConst(t) => (
                Dynamic::from_ast(&Int::from_big_int(ctx, t)),
                Type::Wazzi(WazziType::Int),
            ),
            | Term::IntWrap(t) => {
                let (op, op_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.op, function, params,
                );

                (
                    Dynamic::from_ast(&op_type.unwrap_ast_as_int(types, &op)),
                    Type::Wazzi(WazziType::Int),
                )
            },
            | Term::IntAdd(t) => {
                let (lhs, lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.lhs, function, params,
                );
                let (rhs, rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.rhs, function, params,
                );

                (
                    Dynamic::from_ast(&Int::add(
                        ctx,
                        &[
                            &lhs_type.unwrap_ast_as_int(types, &lhs),
                            &rhs_type.unwrap_ast_as_int(types, &rhs),
                        ],
                    )),
                    Type::Wazzi(WazziType::Int),
                )
            },
            | Term::IntGt(t) => {
                let (lhs, _lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.lhs, function, params,
                );
                let (rhs, _rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.rhs, function, params,
                );

                (
                    Dynamic::from_ast(&lhs.as_int().unwrap().gt(&rhs.as_int().unwrap())),
                    Type::Wazzi(WazziType::Bool),
                )
            },
            | Term::IntLe(t) => {
                let (lhs, lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.lhs, function, params,
                );
                let (rhs, rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.rhs, function, params,
                );

                (
                    Dynamic::from_ast(
                        &lhs_type
                            .unwrap_ast_as_int(types, &lhs)
                            .le(&rhs_type.unwrap_ast_as_int(types, &rhs)),
                    ),
                    Type::Wazzi(WazziType::Bool),
                )
            },
            | Term::ListLen(t) => {
                let (op, op_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.op, function, params,
                );
                let tdef = op_type.wasi().unwrap();
                let datatype = types.resources.get(&tdef.name).unwrap();

                // TODO(yage)
                if tdef.wasi == WasiType::String {
                    (
                        Dynamic::from_ast(
                            &datatype.variants[0].accessors[0]
                                .apply(&[&op])
                                .as_seq()
                                .unwrap()
                                .length(),
                        ),
                        Type::Wazzi(WazziType::Int),
                    )
                } else {
                    (
                        Dynamic::from_ast(
                            &datatype.variants[0].accessors[0]
                                .apply(&[&op])
                                .as_seq()
                                .unwrap()
                                .length(),
                        ),
                        Type::Wazzi(WazziType::Int),
                    )
                }
            },
            | Term::U64Const(t) => {
                let (value, _type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.term, function, params,
                );

                let datatype = types.resources.get("u64").unwrap();

                (
                    datatype.variants[0].constructor.apply(&[&value]),
                    Type::Wasi(spec.types.get_by_key("u64").unwrap().clone()),
                )
            },
            | Term::StrAt(t) => {
                let (mut s, s_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.lhs, function, params,
                );
                let (i, _type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.rhs, function, params,
                );

                if let Type::Wasi(tdef) = s_type {
                    let datatype = types.resources.get(&tdef.name).unwrap();

                    s = datatype.variants[0].accessors[0].apply(&[&s]);
                }

                (
                    Dynamic::from_ast(&s.as_string().unwrap().at(&i.as_int().unwrap())),
                    Type::Wazzi(WazziType::String),
                )
            },
            | Term::ValueEq(t) => {
                let (lhs, _lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.lhs, function, params,
                );
                let (rhs, _rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, &t.rhs, function, params,
                );

                (
                    Dynamic::from_ast(&lhs._eq(&rhs)),
                    Type::Wazzi(WazziType::Bool),
                )
            },
            | Term::VariantConst(t) => {
                let datatype = types.resources.get(&t.ty).unwrap();
                let variant_tdef = spec.types.get_by_key(&t.ty).unwrap();
                let variant_type = variant_tdef.wasi.variant().unwrap();
                let (i, _case) = variant_type
                    .cases
                    .iter()
                    .enumerate()
                    .find(|(_i, case)| case.name == t.case)
                    .unwrap();
                let payload = match &t.payload {
                    | Some(payload_term) => {
                        let (payload, _payload_tdef) = self.term_to_z3_ast(
                            ctx,
                            env,
                            eval_ctx,
                            spec,
                            types,
                            decls,
                            payload_term,
                            function,
                            params,
                        );

                        vec![payload]
                    },
                    | None => vec![],
                };

                (
                    datatype.variants[i].constructor.apply(
                        payload
                            .iter()
                            .map(|p| p as &dyn z3::ast::Ast)
                            .collect_vec()
                            .as_slice(),
                    ),
                    Type::Wasi(variant_tdef.to_owned()),
                )
            },
            | Term::FsFileTypeGet(t) => {
                let (fd_param_idx, _fd_function_param) = function
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.name == t.fd)
                    .unwrap();
                let (path_param_idx, _path_function_param) = function
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.name == t.path)
                    .unwrap();
                let (_fd_value, fd_resource_idx) = params.unwrap().get(fd_param_idx).unwrap();
                let (path_value, _path_resource_idx) = params.unwrap().get(path_param_idx).unwrap();
                let fd_resource_idx = fd_resource_idx.unwrap();
                let mut fd_resource = env.resources.get(fd_resource_idx).unwrap();
                let fd_tdef = spec.types.get_by_key("fd").unwrap();
                let fd_type = fd_tdef.state.as_ref().unwrap().record().unwrap();
                let (parent_member_idx, _parent_member_type) = fd_type
                    .members
                    .iter()
                    .enumerate()
                    .find(|(_i, member)| member.name == "parent")
                    .unwrap();
                let (path_member_idx, _path_member_type) = fd_type
                    .members
                    .iter()
                    .enumerate()
                    .find(|(_i, member)| member.name == "path")
                    .unwrap();
                let mut curr_fd_resource_idx = fd_resource_idx;
                let mut paths =
                    vec![String::from_utf8(path_value.string().unwrap().to_vec()).unwrap()];

                loop {
                    let parent_resource_idx = ResourceIdx(
                        fd_resource
                            .state
                            .record()
                            .unwrap()
                            .members
                            .get(parent_member_idx)
                            .unwrap()
                            .u64()
                            .unwrap() as usize,
                    );

                    if curr_fd_resource_idx == parent_resource_idx {
                        break;
                    }

                    paths.push(
                        String::from_utf8(
                            fd_resource
                                .state
                                .record()
                                .unwrap()
                                .members
                                .get(path_member_idx)
                                .unwrap()
                                .string()
                                .unwrap()
                                .to_vec(),
                        )
                        .unwrap(),
                    );
                    curr_fd_resource_idx = parent_resource_idx;
                    fd_resource = env.resources.get(curr_fd_resource_idx).unwrap();
                }

                let preopen_fd_resource_idx = curr_fd_resource_idx;
                let (_preopen_resource_idx, preopen) = decls
                    .preopens
                    .iter()
                    .find(|&(&resource_idx, _preopen)| resource_idx == preopen_fd_resource_idx)
                    .unwrap();
                let file = FileEncodingRef::Directory(&preopen.root);
                let mut files = vec![file];

                for path in paths.iter() {
                    let path = Path::new(path);

                    for component in path.components() {
                        let filename =
                            String::from_utf8(component.as_os_str().as_encoded_bytes().to_vec())
                                .unwrap();
                        let f = files.last().unwrap();

                        match filename.as_str() {
                            | ".." => {
                                files.pop();
                            },
                            | "." => (),
                            | filename => match f {
                                | FileEncodingRef::Directory(d) => {
                                    files.push(d.children.get(filename).unwrap().as_ref())
                                },
                                | FileEncodingRef::RegularFile(_f) => unreachable!(),
                            },
                        }
                    }
                }

                let filetype_tdef = spec.types.get_by_key("filetype").unwrap();
                let case_idx = match files.last().unwrap() {
                    | FileEncodingRef::Directory(_d) => {
                        filetype_tdef
                            .wasi
                            .variant()
                            .unwrap()
                            .cases
                            .iter()
                            .enumerate()
                            .find(|(_i, case)| case.name == "directory")
                            .unwrap()
                            .0
                    },
                    | FileEncodingRef::RegularFile(_f) => {
                        filetype_tdef
                            .wasi
                            .variant()
                            .unwrap()
                            .cases
                            .iter()
                            .enumerate()
                            .find(|(_i, case)| case.name == "regular_file")
                            .unwrap()
                            .0
                    },
                };

                (
                    types.resources.get("filetype").unwrap().variants[case_idx]
                        .constructor
                        .apply(&[]),
                    Type::Wasi(filetype_tdef.to_owned()),
                )
            },
            | Term::NoNonExistentDirBacktrack(_t) => (
                no_nonexistent_dir_backtrack(ctx, types, decls /*t*/),
                Type::Wazzi(WazziType::Bool),
            ),
        }
    }

    fn decode_to_wasi_value<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        types: &StateTypes,
        tdef: &TypeDef,
        decl: &ParamDecl<'ctx>,
        model: &z3::Model<'ctx>,
    ) -> WasiValue {
        let datatype = types.resources.get(&tdef.name).unwrap();
        let wasi_type = match &tdef.state {
            | Some(t) => t,
            | None => &tdef.wasi,
        };

        match wasi_type {
            | WasiType::S64 => WasiValue::S64(
                datatype.variants[0].accessors[0]
                    .apply(&[decl.node()])
                    .simplify()
                    .as_int()
                    .unwrap()
                    .as_i64()
                    .unwrap(),
            ),
            | WasiType::U8 => WasiValue::U8(
                datatype.variants[0].accessors[0]
                    .apply(&[decl.node()])
                    .simplify()
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => {
                let i = datatype.variants[0].accessors[0]
                    .apply(&[decl.node()])
                    .simplify()
                    .as_int()
                    .unwrap();

                WasiValue::U32(i.as_u64().unwrap() as u32)
            },
            | WasiType::U64 => WasiValue::U64(
                model
                    .eval(
                        &datatype.variants[0].accessors[0].apply(&[decl.node()]),
                        true,
                    )
                    .unwrap()
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
            ),
            | WasiType::Handle => todo!(),
            | WasiType::Flags(flags) => WasiValue::Flags(FlagsValue {
                fields: flags
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, _field)| {
                        model
                            .eval(
                                &datatype.variants[0].accessors[i].apply(&[decl.node()]),
                                true,
                            )
                            .unwrap()
                            .as_bool()
                            .unwrap()
                            .as_bool()
                            .unwrap()
                    })
                    .collect_vec(),
            }),
            | WasiType::Variant(variant) => {
                let mut case_idx = 0;

                for (i, variant) in datatype.variants.iter().enumerate() {
                    if model
                        .eval(&variant.tester.apply(&[decl.node()]), true)
                        .unwrap()
                        .as_bool()
                        .unwrap()
                        .as_bool()
                        .unwrap()
                    {
                        case_idx = i;
                        break;
                    }
                }

                let payload = match &variant.cases[case_idx].payload {
                    | Some(payload_tref) => Some(self.decode_to_wasi_value(
                        ctx,
                        spec,
                        types,
                        payload_tref.resolve(spec),
                        &ParamDecl::Node(
                            datatype.variants[case_idx].accessors[0].apply(&[decl.node()]),
                        ),
                        model,
                    )),
                    | None => None,
                };

                WasiValue::Variant(Box::new(VariantValue { case_idx, payload }))
            },
            | WasiType::Record(record) => WasiValue::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .enumerate()
                    .map(|(i, member)| {
                        self.decode_to_wasi_value(
                            ctx,
                            spec,
                            types,
                            member.tref.resolve(spec),
                            &ParamDecl::Node(
                                datatype.variants[0].accessors[i].apply(&[decl.node()]),
                            ),
                            model,
                        )
                    })
                    .collect_vec(),
            }),
            | WasiType::String => {
                let mut string = String::new();
                let segments = match decl {
                    | ParamDecl::Path { segments } => segments,
                    | ParamDecl::Node(node) => {
                        let seq = model
                            .eval(
                                &types.resources.get("path").unwrap().variants[0].accessors[0]
                                    .apply(&[node]),
                                true,
                            )
                            .unwrap()
                            .as_seq()
                            .unwrap();
                        let mut s = String::new();

                        for i in 0..model.eval(&seq.length(), true).unwrap().as_u64().unwrap() {
                            let seg = seq.nth(&Int::from_u64(ctx, i));

                            if types.segment.variants[0]
                                .tester
                                .apply(&[&seg])
                                .as_bool()
                                .unwrap()
                                .as_bool()
                                .unwrap()
                            {
                                s.push('/');
                            } else if types.segment.variants[1]
                                .tester
                                .apply(&[&seg])
                                .as_bool()
                                .unwrap()
                                .as_bool()
                                .unwrap()
                            {
                                s.push_str(
                                    types.segment.variants[1].accessors[0]
                                        .apply(&[&seg])
                                        .as_string()
                                        .unwrap()
                                        .as_string()
                                        .unwrap()
                                        .as_str(),
                                );
                            }
                        }

                        return WasiValue::String(s.into_bytes());
                    },
                };

                for i in 0..segments.len() {
                    let segment = &segments[i];

                    if model
                        .eval(
                            &types.segment.variants[0]
                                .tester
                                .apply(&[segment])
                                .as_bool()
                                .unwrap(),
                            true,
                        )
                        .unwrap()
                        .as_bool()
                        .unwrap()
                    {
                        string.push('/');
                    } else {
                        string.push_str(
                            model
                                .eval(
                                    &types.segment.variants[1].accessors[0].apply(&[segment]),
                                    true,
                                )
                                .unwrap()
                                .as_string()
                                .unwrap()
                                .as_string()
                                .unwrap()
                                .as_str(),
                        );
                    }
                }

                WasiValue::String(string.into_bytes())
            },
            | WasiType::Pointer(pointer) => {
                let seq = datatype.variants[0].accessors[0]
                    .apply(&[decl.node()])
                    .simplify()
                    .as_seq()
                    .unwrap();
                let length = seq.length().simplify().as_u64().unwrap();
                let mut items = Vec::with_capacity(length as usize);

                for i in 0..length {
                    items.push(self.decode_to_wasi_value(
                        ctx,
                        spec,
                        types,
                        &pointer.item.resolve(spec),
                        &ParamDecl::Node(seq.nth(&Int::from_u64(ctx, i)).simplify()),
                        model,
                    ));
                }

                WasiValue::Pointer(PointerValue { items })
            },
            | WasiType::List(list_type) => {
                let seq = datatype.variants[0].accessors[0]
                    .apply(&[decl.node()])
                    .simplify()
                    .as_seq()
                    .unwrap();
                let length = seq.length().simplify().as_u64().unwrap();
                let mut items = Vec::with_capacity(length as usize);

                for i in 0..length {
                    items.push(self.decode_to_wasi_value(
                        ctx,
                        spec,
                        types,
                        &list_type.item.resolve(spec),
                        &ParamDecl::Node(seq.nth(&Int::from_u64(ctx, i)).simplify()),
                        model,
                    ));
                }

                WasiValue::List(ListValue { items })
            },
        }
    }
}

enum ArbitraryOrPresolved<'u, 'data> {
    Arbitrary(&'u mut Unstructured<'data>),
    Presolved(BTreeMap<String, usize>),
}

#[derive(Debug)]
pub(crate) struct StateTypes<'ctx> {
    resources: BTreeMap<String, z3::DatatypeSort<'ctx>>,
    file:      z3::DatatypeSort<'ctx>,
    segment:   z3::DatatypeSort<'ctx>,
}

impl<'ctx> StateTypes<'ctx> {
    fn new(ctx: &'ctx z3::Context, spec: &Spec) -> Self {
        let mut resources = BTreeMap::new();

        fn encode_type<'ctx>(
            ctx: &'ctx z3::Context,
            spec: &Spec,
            name: &str,
            tdef: &TypeDef,
            resource_types: &mut BTreeMap<String, z3::DatatypeSort<'ctx>>,
        ) {
            if resource_types.get(name).is_some() {
                return;
            }

            let wasi_type = match &tdef.state {
                | Some(state) => state,
                | None => &tdef.wasi,
            };
            let mut datatype = z3::DatatypeBuilder::new(ctx, name);

            datatype = match wasi_type {
                | WasiType::S64
                | WasiType::U8
                | WasiType::U16
                | WasiType::U32
                | WasiType::U64
                | WasiType::Handle => datatype.variant(
                    name,
                    vec![(name, z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                ),
                | WasiType::Flags(flags_type) => datatype.variant(
                    name,
                    flags_type
                        .fields
                        .iter()
                        .map(|field| {
                            (
                                field.as_str(),
                                z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)),
                            )
                        })
                        .collect_vec(),
                ),
                | WasiType::Variant(variant_type) => {
                    for case in &variant_type.cases {
                        let fields = match &case.payload {
                            | Some(payload) => {
                                let payload_tdef = payload.resolve(spec);

                                encode_type(
                                    ctx,
                                    spec,
                                    &payload_tdef.name,
                                    payload_tdef,
                                    resource_types,
                                );

                                vec![(
                                    "payload",
                                    z3::DatatypeAccessor::Sort(
                                        resource_types
                                            .get(&payload_tdef.name)
                                            .unwrap()
                                            .sort
                                            .clone(),
                                    ),
                                )]
                            },
                            | None => vec![],
                        };

                        datatype = datatype.variant(&case.name, fields);
                    }

                    datatype
                },
                | WasiType::Record(record_type) => datatype.variant(
                    name,
                    record_type
                        .members
                        .iter()
                        .map(|member| {
                            let member_tdef = member.tref.resolve(spec);

                            encode_type(
                                ctx,
                                spec,
                                &member_tdef.name,
                                member.tref.resolve(spec),
                                resource_types,
                            );

                            let member_datatype =
                                resource_types.get(&member.tref.resolve(spec).name).unwrap();
                            (
                                member.name.as_str(),
                                z3::DatatypeAccessor::Sort(member_datatype.sort.clone()),
                            )
                        })
                        .collect_vec(),
                ),
                | WasiType::String => unimplemented!(),
                | WasiType::Pointer(pointer) => {
                    let tdef = pointer.item.resolve(spec);

                    datatype.variant(
                        name,
                        vec![(
                            name,
                            z3::DatatypeAccessor::Sort(z3::Sort::seq(
                                ctx,
                                &resource_types.get(&tdef.name).unwrap().sort,
                            )),
                        )],
                    )
                },
                | WasiType::List(list_type) => {
                    let tdef = list_type.item.resolve(spec);

                    datatype.variant(
                        name,
                        vec![(
                            name,
                            z3::DatatypeAccessor::Sort(z3::Sort::seq(
                                ctx,
                                &resource_types.get(&tdef.name).unwrap().sort,
                            )),
                        )],
                    )
                },
            };

            resource_types.insert(tdef.name.clone(), datatype.finish());
        }

        let segment_type = z3::DatatypeBuilder::new(ctx, "segment")
            .variant("separator", vec![])
            .variant(
                "component",
                vec![(
                    "component-str",
                    z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)),
                )],
            )
            .finish();

        for (name, tdef) in spec.types.iter() {
            if name == "path" {
                resources.insert(
                    "path".to_string(),
                    z3::DatatypeBuilder::new(ctx, "path")
                        .variant(
                            "path",
                            vec![(
                                "segments",
                                z3::DatatypeAccessor::Sort(z3::Sort::seq(ctx, &segment_type.sort)),
                            )],
                        )
                        .finish(),
                );

                continue;
            }

            encode_type(ctx, spec, name, tdef, &mut resources);
        }

        Self {
            resources,
            file: z3::DatatypeBuilder::new(ctx, "file")
                .variant(
                    "directory",
                    vec![(
                        "directory-id",
                        z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)),
                    )],
                )
                .variant(
                    "regular-file",
                    vec![(
                        "regular-file-id",
                        z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)),
                    )],
                )
                .finish(),
            segment: segment_type,
        }
    }

    fn encode_wasi_value(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        param: &ParamDecl<'ctx>,
        tdef: &TypeDef,
        value: &WasiValue,
    ) -> Bool<'ctx> {
        let datatype = self.resources.get(&tdef.name).unwrap();
        let ty = match &tdef.state {
            | Some(t) => t,
            | None => &tdef.wasi,
        };

        match (ty, value) {
            | (_, &WasiValue::U8(i)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U16(i)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U32(i)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U64(i)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_u64(ctx, i)),
            | (_, &WasiValue::S64(i)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_i64(ctx, i)),
            | (_, &WasiValue::Handle(handle)) => datatype.variants[0].accessors[0]
                .apply(&[param.node()])
                .as_int()
                .unwrap()
                ._eq(&Int::from_u64(ctx, handle.into())),
            | (WasiType::Record(record), WasiValue::Record(record_value)) => Bool::and(
                ctx,
                record
                    .members
                    .iter()
                    .enumerate()
                    .zip(record_value.members.iter())
                    .map(|((i, member), member_value)| {
                        self.encode_wasi_value(
                            ctx,
                            spec,
                            &ParamDecl::Node(
                                datatype.variants[0].accessors[i].apply(&[param.node()]),
                            ),
                            member.tref.resolve(spec),
                            member_value,
                        )
                    })
                    .collect_vec()
                    .as_slice(),
            ),
            | (_, WasiValue::Record(_)) => unreachable!(),
            | (WasiType::Flags(flags), WasiValue::Flags(flags_value)) => Bool::and(
                ctx,
                flags
                    .fields
                    .iter()
                    .enumerate()
                    .zip(flags_value.fields.iter())
                    .map(|((i, _name), &value)| {
                        datatype.variants[0].accessors[i]
                            .apply(&[param.node()])
                            .as_bool()
                            .unwrap()
                            ._eq(&Bool::from_bool(ctx, value))
                    })
                    .collect_vec()
                    .as_slice(),
            ),
            | (_, WasiValue::Flags(_)) => unreachable!(),
            | (_, WasiValue::String(string)) => {
                let seq = datatype.variants[0].accessors[0]
                    .apply(&[param.node()])
                    .as_seq()
                    .unwrap();
                let s = String::from_utf8(string.to_owned()).unwrap();
                let mut segments = Vec::new();
                let mut i = 0;
                let mut last_i = 0;

                while i < s.len() {
                    if s.as_bytes()[i] != b'/' {
                        last_i = i;
                        i += 1;
                        continue;
                    }

                    if last_i < i {
                        segments.push(Segment::Component(&s[last_i..i]));
                    }

                    segments.push(Segment::Separator);
                    last_i = i;
                    i += 1;
                }

                if last_i < i {
                    segments.push(Segment::Component(&s[last_i..i]));
                }

                Bool::and(
                    ctx,
                    &[
                        seq.length()._eq(&Int::from_u64(ctx, segments.len() as u64)),
                        Bool::and(
                            ctx,
                            segments
                                .into_iter()
                                .enumerate()
                                .map(|(i, segment)| match segment {
                                    | Segment::Separator => self.segment.variants[0]
                                        .tester
                                        .apply(&[&seq.nth(&Int::from_u64(ctx, i as u64))])
                                        .as_bool()
                                        .unwrap(),
                                    | Segment::Component(s) => self.segment.variants[1].accessors
                                        [0]
                                    .apply(&[&seq.nth(&Int::from_u64(ctx, i as u64))])
                                    .as_string()
                                    .unwrap()
                                    ._eq(&z3::ast::String::from_str(ctx, s).unwrap()),
                                })
                                .collect_vec()
                                .as_slice(),
                        ),
                    ],
                )
            },
            | (WasiType::Variant(variant), WasiValue::Variant(variant_value)) => {
                match &variant_value.payload {
                    | Some(payload) => {
                        let payload_tdef = variant.cases[variant_value.case_idx]
                            .payload
                            .as_ref()
                            .unwrap()
                            .resolve(spec);

                        Bool::and(
                            ctx,
                            &[
                                datatype.variants[variant_value.case_idx]
                                    .tester
                                    .apply(&[param.node()])
                                    .as_bool()
                                    .unwrap(),
                                self.encode_wasi_value(
                                    ctx,
                                    spec,
                                    &ParamDecl::Node(
                                        datatype.variants[variant_value.case_idx].accessors[0]
                                            .apply(&[param.node()]),
                                    ),
                                    payload_tdef,
                                    payload,
                                ),
                            ],
                        )
                    },
                    | None => datatype.variants[variant_value.case_idx]
                        .tester
                        .apply(&[param.node()])
                        .as_bool()
                        .unwrap(),
                }
            },
            | (_, WasiValue::Variant(_variant_value)) => unreachable!(),
            | (WasiType::Pointer(pointer), WasiValue::Pointer(pointer_value)) => {
                let idx = Int::fresh_const(ctx, "");

                forall_const(
                    ctx,
                    &[&idx],
                    &[],
                    &Bool::and(
                        ctx,
                        &[
                            Int::from_u64(ctx, 0).le(&idx),
                            idx.lt(&Int::from_u64(ctx, pointer_value.items.len() as u64)),
                        ],
                    )
                    .implies(&Bool::and(
                        ctx,
                        (0..pointer_value.items.len())
                            .map(|i| {
                                idx._eq(&Int::from_u64(ctx, i as u64)).implies(
                                    &self.encode_wasi_value(
                                        ctx,
                                        spec,
                                        &ParamDecl::Node(
                                            datatype.variants[0].accessors[0]
                                                .apply(&[param.node()])
                                                .as_seq()
                                                .unwrap()
                                                .nth(&idx),
                                        ),
                                        &pointer.item.resolve(spec),
                                        pointer_value.items.get(i).unwrap(),
                                    ),
                                )
                            })
                            .collect_vec()
                            .as_slice(),
                    )),
                )
            },
            | (_, WasiValue::Pointer(_pointer_value)) => unreachable!(),
            | (WasiType::List(list), WasiValue::List(list_value)) => {
                let idx = Int::fresh_const(ctx, "");

                forall_const(
                    ctx,
                    &[&idx],
                    &[],
                    &Bool::and(
                        ctx,
                        &[
                            Int::from_u64(ctx, 0).le(&idx),
                            idx.lt(&Int::from_u64(ctx, list_value.items.len() as u64)),
                        ],
                    )
                    .implies(&Bool::and(
                        ctx,
                        (0..list_value.items.len())
                            .map(|i| {
                                idx._eq(&Int::from_u64(ctx, i as u64)).implies(
                                    &self.encode_wasi_value(
                                        ctx,
                                        spec,
                                        &ParamDecl::Node(
                                            datatype.variants[0].accessors[0]
                                                .apply(&[param.node()])
                                                .as_seq()
                                                .unwrap()
                                                .nth(&idx),
                                        ),
                                        &list.item.resolve(spec),
                                        list_value.items.get(i).unwrap(),
                                    ),
                                )
                            })
                            .collect_vec()
                            .as_slice(),
                    )),
                )
            },
            | (_, WasiValue::List(_list_value)) => unreachable!("{:#?}", _list_value),
        }
    }
}

fn no_nonexistent_dir_backtrack<'ctx>(
    ctx: &'ctx z3::Context,
    _types: &'ctx StateTypes<'ctx>,
    decls: &'ctx StateDecls<'ctx>,
    // t: &NoNonExistentDirBacktrack,
) -> Dynamic<'ctx> {
    // let mut clauses: Vec<Bool> = Vec::new();
    // let namespace = format!("nndb-{}-{}", t.fd_param, t.path_param);
    // let segment_file = z3::FuncDecl::new(
    //     ctx,
    //     format!("{namespace}--segment-file"),
    //     &[&z3::Sort::int(ctx), &types.file.sort],
    //     &z3::Sort::bool(ctx),
    // );
    // let param_path = decls.params.get(&t.path_param).unwrap();
    // let param_fd = decls.params.get(&t.fd_param).unwrap();

    // {
    //     let mut all_files = decls
    //         .preopens
    //         .values()
    //         .map(|preopen| &preopen.root.node)
    //         .collect_vec();

    //     for (_idx, preopen) in decls.preopens.iter() {
    //         let mut stack = vec![&preopen.root];

    //         while let Some(dir) = stack.pop() {
    //             for (_filename, child) in dir.children.iter() {
    //                 all_files.push(child.node());

    //                 match child {
    //                     | FileEncoding::Directory(d) => stack.push(d),
    //                     | FileEncoding::RegularFile(_f) => continue,
    //                 }
    //             }
    //         }
    //     }

    //     let some_file = Dynamic::fresh_const(ctx, "", &types.file.sort);
    //     let some_segment = Dynamic::fresh_const(ctx, "", &types.segment.sort);

    //     clauses.push(forall_const(
    //         ctx,
    //         &[&some_file, &some_segment],
    //         &[],
    //         &Bool::and(
    //             ctx,
    //             all_files
    //                 .into_iter()
    //                 .map(|file| file._eq(&some_file).not())
    //                 .collect_vec()
    //                 .as_slice(),
    //         )
    //         .implies(
    //             &segment_file
    //                 .apply(&[&some_segment, &some_file])
    //                 .as_bool()
    //                 .unwrap()
    //                 .not(),
    //         ),
    //     ));
    // }

    for (&_idx, preopen) in decls.preopens.iter() {
        let mut stack = vec![&preopen.root];
        // let path_datatype = types.resources.get("path").unwrap();
        // let segments_accessor = &path_datatype.variants[0].accessors[0];
        // let some_idx = Int::fresh_const(ctx, "");

        // clauses.push(forall_const(
        //     ctx,
        //     &[&some_idx],
        //     &[],
        //     &segment_file
        //         .apply(&[&some_idx, &preopen.root.node])
        //         .as_bool()
        //         .unwrap()
        //         .not(),
        // ));

        while let Some(dir) = stack.pop() {
            // clauses.push(
            //     decls
            //         .fd_file
            //         .apply(&[param_fd, &dir.node])
            //         .as_bool()
            //         .unwrap()
            //         .ite(
            //             &Bool::and(
            //                 ctx,
            //                 dir.children
            //                     .iter()
            //                     .map(|(filename, child)| {
            //                         Bool::and(
            //                             ctx,
            //                             &[
            //                                 decls
            //                                     .children
            //                                     .apply(&[
            //                                         &dir.node,
            //                                         &z3::ast::String::from_str(ctx, filename)
            //                                             .unwrap(),
            //                                         child.node(),
            //                                     ])
            //                                     .as_bool()
            //                                     .unwrap(),
            //                                 types.segment.variants[1].accessors[0]
            //                                     .apply(&[&segments_accessor
            //                                         .apply(&[param_path])
            //                                         .as_seq()
            //                                         .unwrap()
            //                                         .nth(&Int::from_u64(ctx, 0))])
            //                                     .as_string()
            //                                     .unwrap()
            //                                     ._eq(
            //                                         &z3::ast::String::from_str(ctx, filename)
            //                                             .unwrap(),
            //                                     ),
            //                             ],
            //                         )
            //                         .iff(
            //                             &segment_file
            //                                 .apply(&[
            //                                     &segments_accessor
            //                                         .apply(&[param_path])
            //                                         .as_seq()
            //                                         .unwrap()
            //                                         .nth(&Int::from_u64(ctx, 0)),
            //                                     child.node(),
            //                                 ])
            //                                 .as_bool()
            //                                 .unwrap(),
            //                         )
            //                     })
            //                     .collect_vec()
            //                     .as_slice(),
            //             ),
            //             &Bool::or(
            //                 ctx,
            //                 dir.children
            //                     .iter()
            //                     .map(|(_filename, child)| {
            //                         segment_file
            //                             .apply(&[
            //                                 &segments_accessor
            //                                     .apply(&[param_path])
            //                                     .as_seq()
            //                                     .unwrap()
            //                                     .nth(&Int::from_u64(ctx, 0)),
            //                                 child.node(),
            //                             ])
            //                             .as_bool()
            //                             .unwrap()
            //                     })
            //                     .collect_vec()
            //                     .as_slice(),
            //             )
            //             .not(),
            //         ),
            // );

            for (_filename, file) in dir.children.iter() {
                // let some_file = Dynamic::fresh_const(ctx, "", &types.file.sort);
                // let some_string = z3::ast::String::fresh_const(ctx, "");
                // let some_prev_idx = Int::fresh_const(ctx, "");
                // let some_between_idx = Int::fresh_const(ctx, "");

                // clauses.push(forall_const(
                //     ctx,
                //     &[&some_idx],
                //     &[],
                //     &Bool::and(
                //         ctx,
                //         &[
                //             decls
                //                 .fd_file
                //                 .apply(&[param_fd, &dir.node])
                //                 .as_bool()
                //                 .unwrap(),
                //             // Constrain i bounds..
                //             Int::from_u64(ctx, 0).le(&some_idx),
                //             some_idx.lt(&segments_accessor
                //                 .apply(&[param_path])
                //                 .as_seq()
                //                 .unwrap()
                //                 .length()),
                //             // segment[i] is a component.
                //             types.segment.variants[1]
                //                 .tester
                //                 .apply(&[&segments_accessor
                //                     .apply(&[param_path])
                //                     .as_seq()
                //                     .unwrap()
                //                     .nth(&some_idx)])
                //                 .as_bool()
                //                 .unwrap(),
                //         ],
                //     )
                //     .implies(&Bool::or(
                //         ctx,
                //         &[
                //             Bool::and(
                //                 ctx,
                //                 &[
                //                     // segment[i] is `..`
                //                     types.segment.variants[1].accessors[0]
                //                         .apply(&[&segments_accessor
                //                             .apply(&[param_path])
                //                             .as_seq()
                //                             .unwrap()
                //                             .nth(&some_idx)])
                //                         .as_string()
                //                         .unwrap()
                //                         ._eq(&z3::ast::String::from_str(ctx, "..").unwrap()),
                //                     exists_const(
                //                         ctx,
                //                         &[&some_file, &some_string],
                //                         &[],
                //                         &Bool::and(
                //                             ctx,
                //                             &[
                //                                 segment_file
                //                                     .apply(&[&some_idx, &some_file])
                //                                     .as_bool()
                //                                     .unwrap(),
                //                                 decls
                //                                     .children
                //                                     .apply(&[&some_file, &some_string, &dir.node])
                //                                     .as_bool()
                //                                     .unwrap(),
                //                             ],
                //                         ),
                //                     ),
                //                 ],
                //             ),
                //             Bool::and(
                //                 ctx,
                //                 &[
                //                     types.segment.variants[1].accessors[0]
                //                         .apply(&[&segments_accessor
                //                             .apply(&[param_path])
                //                             .as_seq()
                //                             .unwrap()
                //                             .nth(&some_idx)])
                //                         .as_string()
                //                         .unwrap()
                //                         ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                //                     segment_file
                //                         .apply(&[&some_idx, &dir.node])
                //                         .as_bool()
                //                         .unwrap(),
                //                 ],
                //             ),
                //             Bool::and(
                //                 ctx,
                //                 &[
                //                     // segment[i] is the filename
                //                     types.segment.variants[1].accessors[0]
                //                         .apply(&[&segments_accessor
                //                             .apply(&[param_path])
                //                             .as_seq()
                //                             .unwrap()
                //                             .nth(&some_idx)])
                //                         .as_string()
                //                         .unwrap()
                //                         ._eq(&z3::ast::String::from_str(ctx, &filename).unwrap()),
                //                     segment_file
                //                         .apply(&[&some_idx, file.node()])
                //                         .as_bool()
                //                         .unwrap(),
                //                 ],
                //             ),
                //             // segment[i] is the filename
                //             types.segment.variants[1].accessors[0]
                //                 .apply(&[&segments_accessor
                //                     .apply(&[param_path])
                //                     .as_seq()
                //                     .unwrap()
                //                     .nth(&some_idx)])
                //                 .as_string()
                //                 .unwrap()
                //                 ._eq(&z3::ast::String::from_str(ctx, &filename).unwrap())
                //                 .not(),
                //         ],
                //     )),
                // ));
                // clauses.push(forall_const(
                //     ctx,
                //     &[&some_idx],
                //     &[],
                //     &exists_const(
                //         ctx,
                //         &[&some_prev_idx],
                //         &[],
                //         &Bool::and(
                //             ctx,
                //             &[
                //                 decls
                //                     .fd_file
                //                     .apply(&[param_fd, &dir.node])
                //                     .as_bool()
                //                     .unwrap(),
                //                 // Constrain i bounds..
                //                 Int::from_u64(ctx, 0).le(&some_idx),
                //                 some_idx.lt(&segments_accessor
                //                     .apply(&[param_path])
                //                     .as_seq()
                //                     .unwrap()
                //                     .length()),
                //                 // Constrain prev_i bounds..
                //                 Int::from_u64(ctx, 0).le(&some_idx),
                //                 some_idx.lt(&some_idx),
                //                 // segment[i] is a component.
                //                 types.segment.variants[1]
                //                     .tester
                //                     .apply(&[&segments_accessor
                //                         .apply(&[param_path])
                //                         .as_seq()
                //                         .unwrap()
                //                         .nth(&some_idx)])
                //                     .as_bool()
                //                     .unwrap(),
                //                 // segment[prev_i] is a component.
                //                 types.segment.variants[1]
                //                     .tester
                //                     .apply(&[&segments_accessor
                //                         .apply(&[param_path])
                //                         .as_seq()
                //                         .unwrap()
                //                         .nth(&some_prev_idx)])
                //                     .as_bool()
                //                     .unwrap(),
                //                 // And all segment in between are separators
                //                 forall_const(
                //                     ctx,
                //                     &[&some_between_idx],
                //                     &[],
                //                     &Bool::and(
                //                         ctx,
                //                         &[
                //                             some_prev_idx.lt(&some_between_idx),
                //                             some_between_idx.lt(&some_idx),
                //                             types.segment.variants[0]
                //                                 .tester
                //                                 .apply(&[&segments_accessor
                //                                     .apply(&[param_path])
                //                                     .as_seq()
                //                                     .unwrap()
                //                                     .nth(&some_prev_idx)])
                //                                 .as_bool()
                //                                 .unwrap(),
                //                         ],
                //                     ),
                //                 ),
                //                 // Previous segment doesn't maps to a file
                //                 forall_const(
                //                     ctx,
                //                     &[&some_file],
                //                     &[],
                //                     &segment_file
                //                         .apply(&[&some_prev_idx, &some_file])
                //                         .as_bool()
                //                         .unwrap()
                //                         .not(),
                //                 ),
                //             ],
                //         ),
                //     )
                //     .implies(
                //         &types.segment.variants[1].accessors[0]
                //             .apply(&[&segments_accessor
                //                 .apply(&[param_path])
                //                 .as_seq()
                //                 .unwrap()
                //                 .nth(&some_idx)])
                //             .as_string()
                //             .unwrap()
                //             ._eq(&z3::ast::String::from_str(ctx, "..").unwrap())
                //             .not(),
                //     ),
                // ));

                // for i in 0..param_path.segments.len() {
                //     let segment = param_path.segments.get(i).unwrap();
                //     let some_file = Dynamic::fresh_const(ctx, "", &types.file.sort);
                //     let some_string = z3::ast::String::fresh_const(ctx, "");

                //     // clauses.push(
                //     //     Bool::and(
                //     //         ctx,
                //     //         &[
                //     //             decls
                //     //                 .fd_file
                //     //                 .apply(&[param_fd, &dir.node])
                //     //                 .as_bool()
                //     //                 .unwrap(),
                //     //             // This segment is a component.
                //     //             types.segment.variants[1]
                //     //                 .tester
                //     //                 .apply(&[segment])
                //     //                 .as_bool()
                //     //                 .unwrap(),
                //     //         ],
                //     //     )
                //     //     .implies(&Bool::or(
                //     //         ctx,
                //     //         &[
                //     //             Bool::and(
                //     //                 ctx,
                //     //                 &[
                //     //                     types.segment.variants[1].accessors[0]
                //     //                         .apply(&[segment])
                //     //                         .as_string()
                //     //                         .unwrap()
                //     //                         ._eq(&z3::ast::String::from_str(ctx, "..").unwrap()),
                //     //                     exists_const(
                //     //                         ctx,
                //     //                         &[&some_file, &some_string],
                //     //                         &[],
                //     //                         &Bool::and(
                //     //                             ctx,
                //     //                             &[
                //     //                                 segment_file
                //     //                                     .apply(&[segment, &some_file])
                //     //                                     .as_bool()
                //     //                                     .unwrap(),
                //     //                                 decls
                //     //                                     .children
                //     //                                     .apply(&[
                //     //                                         &some_file,
                //     //                                         &some_string,
                //     //                                         &dir.node,
                //     //                                     ])
                //     //                                     .as_bool()
                //     //                                     .unwrap(),
                //     //                             ],
                //     //                         ),
                //     //                     ),
                //     //                 ],
                //     //             ),
                //     //             Bool::and(
                //     //                 ctx,
                //     //                 &[
                //     //                     types.segment.variants[1].accessors[0]
                //     //                         .apply(&[segment])
                //     //                         .as_string()
                //     //                         .unwrap()
                //     //                         ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                //     //                     segment_file
                //     //                         .apply(&[segment, &dir.node])
                //     //                         .as_bool()
                //     //                         .unwrap(),
                //     //                 ],
                //     //             ),
                //     //             Bool::and(
                //     //                 ctx,
                //     //                 &[
                //     //                     types.segment.variants[1].accessors[0]
                //     //                         .apply(&[segment])
                //     //                         .as_string()
                //     //                         .unwrap()
                //     //                         ._eq(
                //     //                             &z3::ast::String::from_str(ctx, &filename).unwrap(),
                //     //                         ),
                //     //                     segment_file
                //     //                         .apply(&[segment, file.node()])
                //     //                         .as_bool()
                //     //                         .unwrap(),
                //     //                 ],
                //     //             ),
                //     //             types.segment.variants[1].accessors[0]
                //     //                 .apply(&[segment])
                //     //                 .as_string()
                //     //                 .unwrap()
                //     //                 ._eq(&z3::ast::String::from_str(ctx, &filename).unwrap())
                //     //                 .not(),
                //     //         ],
                //     //     )),
                //     // );

                //     for j in 0..i {
                //         let prev_segment = param_path.segments.get(j).unwrap();
                //         let some_prev_file = Dynamic::fresh_const(ctx, "", &types.file.sort);

                //         clauses.push(Bool::and(
                //             ctx,
                //             &[Bool::and(
                //                 ctx,
                //                 &[
                //                     decls
                //                         .fd_file
                //                         .apply(&[param_fd, &dir.node])
                //                         .as_bool()
                //                         .unwrap(),
                //                     // Previous segment is a component.
                //                     types.segment.variants[1]
                //                         .tester
                //                         .apply(&[prev_segment])
                //                         .as_bool()
                //                         .unwrap(),
                //                     // And all segment in between are separators
                //                     z3::ast::Bool::and(
                //                         ctx,
                //                         ((j + 1)..i)
                //                             .map(|k| {
                //                                 let segment_in_between =
                //                                     param_path.segments.get(k).unwrap();

                //                                 types.segment.variants[0]
                //                                     .tester
                //                                     .apply(&[segment_in_between])
                //                                     .as_bool()
                //                                     .unwrap()
                //                             })
                //                             .collect_vec()
                //                             .as_slice(),
                //                     ),
                //                     // Previous segment doesn't maps to a file
                //                     forall_const(
                //                         ctx,
                //                         &[&some_prev_file],
                //                         &[],
                //                         &segment_file
                //                             .apply(&[prev_segment, &some_prev_file])
                //                             .as_bool()
                //                             .unwrap()
                //                             .not(),
                //                     ),
                //                 ],
                //             )
                //             .implies(&Bool::or(
                //                 ctx,
                //                 &[
                //                     &types.segment.variants[0]
                //                         .tester
                //                         .apply(&[segment])
                //                         .as_bool()
                //                         .unwrap(),
                //                     &types.segment.variants[1].accessors[0]
                //                         .apply(&[segment])
                //                         .as_string()
                //                         .unwrap()
                //                         ._eq(&z3::ast::String::from_str(ctx, "..").unwrap())
                //                         .not(),
                //                 ],
                //             ))],
                //         ));
                //     }
                // }

                match file {
                    | FileEncoding::Directory(d) => stack.push(d),
                    | FileEncoding::RegularFile(_f) => continue,
                }
            }
        }
    }

    // Dynamic::from_ast(&Bool::and(ctx, clauses.as_slice()))
    Dynamic::from_ast(&Bool::and(ctx, &[Bool::from_bool(ctx, true)]))
}

#[derive(Debug)]
struct StateDecls<'ctx> {
    fd_file:   z3::FuncDecl<'ctx>,
    children:  z3::FuncDecl<'ctx>,
    preopens:  BTreeMap<ResourceIdx, PreopenFsEncoding<'ctx>>,
    resources: BTreeMap<ResourceIdx, Dynamic<'ctx>>,
    params:    BTreeMap<String, ParamDecl<'ctx>>,
    to_solves: ToSolves<'ctx>,
}

#[derive(Debug)]
enum ParamDecl<'ctx> {
    Node(Dynamic<'ctx>),
    Path { segments: Vec<Dynamic<'ctx>> },
}

impl<'ctx> ParamDecl<'ctx> {
    fn node(&self) -> &Dynamic<'ctx> {
        match self {
            | ParamDecl::Node(n) => n,
            | ParamDecl::Path { .. } => panic!(),
        }
    }
}

pub struct StatefulStrategy<'u, 'data, 'ctx, 'zctx> {
    z3_ctx: &'zctx z3::Context,
    u:      &'u mut Unstructured<'data>,
    ctx:    &'ctx RuntimeContext,
}

impl<'u, 'data, 'ctx, 'zctx> StatefulStrategy<'u, 'data, 'ctx, 'zctx> {
    pub fn new(
        u: &'u mut Unstructured<'data>,
        ctx: &'ctx RuntimeContext,
        z3_ctx: &'zctx z3::Context,
    ) -> Self {
        Self { z3_ctx, u, ctx }
    }
}

impl CallStrategy for StatefulStrategy<'_, '_, '_, '_> {
    fn select_function<'spec>(
        &mut self,
        spec: &'spec Spec,
        env: &Environment,
    ) -> Result<&'spec Function, eyre::Error> {
        let interface = spec
            .interfaces
            .get_by_key("wasi_snapshot_preview1")
            .unwrap();
        let mut candidates = Vec::new();

        for (_name, function) in &interface.functions {
            let mut state = State::new();

            for (&idx, path) in &self.ctx.preopens {
                state.push_preopen(idx, path);
            }

            for (resource_type, resources) in &env.resources_by_types {
                for &idx in resources {
                    state.push_resource(
                        idx,
                        spec.types.get_by_key(resource_type).unwrap(),
                        env.resources.get(idx).unwrap().state.clone(),
                    );
                }
            }

            let types = StateTypes::new(self.z3_ctx, spec);
            let decls = state.declare(
                ArbitraryOrPresolved::Arbitrary(self.u),
                spec,
                self.z3_ctx,
                &types,
                env,
                function,
                None,
            );
            let solver = z3::Solver::new(self.z3_ctx);

            solver.assert(&state.encode(
                self.z3_ctx,
                env,
                &types,
                &decls,
                spec,
                function,
                None,
                function.input_contract.as_ref(),
            ));

            match solver.check() {
                | z3::SatResult::Sat => candidates.push(function),
                | _ => continue,
            };
        }

        let function = *self
            .u
            .choose(&candidates)
            .wrap_err("failed to choose a function")?;

        Ok(function)
    }

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &Environment,
    ) -> Result<Vec<(WasiValue, Option<ResourceIdx>)>, eyre::Error> {
        let mut state = State::new();

        for (&idx, path) in &self.ctx.preopens {
            state.push_preopen(idx, path);
        }

        for (resource_type, resources) in &env.resources_by_types {
            for &idx in resources {
                state.push_resource(
                    idx,
                    spec.types.get_by_key(resource_type).unwrap(),
                    env.resources.get(idx).unwrap().state.clone(),
                );
            }
        }

        let types = StateTypes::new(self.z3_ctx, spec);
        let decls = state.declare(
            ArbitraryOrPresolved::Arbitrary(self.u),
            spec,
            self.z3_ctx,
            &types,
            env,
            function,
            None,
        );
        let solver = z3::Solver::new(self.z3_ctx);
        let mut solver_params = z3::Params::new(self.z3_ctx);

        solver_params.set_u32("sat.random_seed", self.u.arbitrary()?);
        solver_params.set_u32("smt.random_seed", self.u.arbitrary()?);
        solver.set_params(&solver_params);

        let mut solutions = Vec::new();
        let mut nsolutions = 0;

        solver.push();
        solver.assert(&state.encode(
            self.z3_ctx,
            &env,
            &types,
            &decls,
            spec,
            function,
            None,
            function.input_contract.as_ref(),
        ));

        loop {
            if solver.check() != z3::SatResult::Sat || nsolutions == 10 {
                break;
            }

            let model = solver.get_model().unwrap();
            let mut clauses = Vec::new();

            for (_param_name, param_decl) in decls.params.iter() {
                match param_decl {
                    | ParamDecl::Node(node) => {
                        let p = model.eval(node, true).unwrap().simplify();

                        clauses.push(node._eq(&p).not());
                    },
                    | ParamDecl::Path { segments } => {
                        clauses.push(Bool::or(
                            self.z3_ctx,
                            segments
                                .iter()
                                .map(|segment| {
                                    let p = model.eval(segment, true).unwrap().simplify();

                                    segment._eq(&p).not()
                                })
                                .collect_vec()
                                .as_slice(),
                        ));
                    },
                }
            }

            solutions.push(model);
            nsolutions += 1;
            solver.assert(&Bool::or(self.z3_ctx, clauses.as_slice()));
        }

        let model = self.u.choose(&solutions).unwrap();
        let mut params = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            let tdef = param.tref.resolve(spec);
            let param_node_value = decls.params.get(&param.name).unwrap();
            let wasi_value = state.decode_to_wasi_value(
                self.z3_ctx,
                spec,
                &types,
                &tdef,
                &param_node_value,
                &model,
            );

            match &tdef.state {
                | Some(_state) => {
                    let x = env.reverse_resource_index.get(&tdef.name).unwrap();
                    let resource_idx = x.get(&wasi_value);
                    let resource_idx = match resource_idx {
                        | Some(resource_idx) => *resource_idx,
                        | None => panic!("{:#?} -> {:#?}", wasi_value, x),
                    };
                    let value = self.ctx.resources.get(&resource_idx).unwrap();

                    params.push((value.to_owned(), Some(resource_idx)));
                },
                | None => params.push((wasi_value, None)),
            }
        }

        Ok(params)
    }

    fn handle_results(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &mut Environment,
        params: Vec<(WasiValue, Option<ResourceIdx>)>,
        results: Vec<Option<ResourceIdx>>,
    ) -> Result<(), eyre::Error> {
        let mut state = State::new();

        for (&idx, path) in &self.ctx.preopens {
            state.push_preopen(idx, path);
        }

        for (resource_type, resources) in &env.resources_by_types {
            for &idx in resources {
                state.push_resource(
                    idx,
                    spec.types.get_by_key(resource_type).unwrap(),
                    env.resources.get(idx).unwrap().state.clone(),
                );
            }
        }

        let lens = function
            .params
            .iter()
            .zip(params.iter())
            .filter(|(function_param, _param)| function_param.tref.resolve(spec).name == "path")
            .map(|(function_param, param)| {
                let s = String::from_utf8(param.0.string().unwrap().to_vec()).unwrap();
                let mut segments = Vec::new();
                let mut i = 0;
                let mut last_i = 0;

                while i < s.len() {
                    if s.as_bytes()[i] != b'/' {
                        last_i = i;
                        i += 1;
                        continue;
                    }

                    if last_i < i {
                        segments.push(Segment::Component(&s[last_i..i]));
                    }

                    segments.push(Segment::Separator);
                    last_i = i;
                    i += 1;
                }

                if last_i < i {
                    segments.push(Segment::Component(&s[last_i..i]));
                }

                (function_param.name.clone(), segments.len())
            })
            .collect();

        let types = StateTypes::new(self.z3_ctx, spec);
        let decls = state.declare(
            ArbitraryOrPresolved::Presolved(lens),
            spec,
            self.z3_ctx,
            &types,
            env,
            function,
            function.output_contract.as_ref(),
        );
        let solver = z3::Solver::new(self.z3_ctx);

        solver.assert(&state.encode(
            self.z3_ctx,
            &env,
            &types,
            &decls,
            spec,
            function,
            Some(&params),
            function.output_contract.as_ref(),
        ));

        // Concretize the param values.
        for (i, function_param) in function.params.iter().enumerate() {
            let tdef = function_param.tref.resolve(spec);
            let value = match &tdef.state {
                | Some(_) => {
                    &env.resources
                        .get(params.get(i).unwrap().1.unwrap())
                        .unwrap()
                        .state
                },
                | None => &params.get(i).unwrap().0,
            };
            let param_node = decls.params.get(&function_param.name).unwrap();

            solver.assert(&types.encode_wasi_value(self.z3_ctx, spec, param_node, &tdef, value));
        }

        match solver.check() {
            | z3::SatResult::Sat => (),
            | _ => return Err(err!("failed to solve output contract")),
        }

        let model = solver.get_model().unwrap();
        let mut clauses = Vec::new();

        for (name, result) in &decls.to_solves.results {
            let result_value = model.eval(result, true).unwrap().simplify();
            let (result_idx, function_result) = function
                .results
                .iter()
                .enumerate()
                .find(|(_, result)| &result.name == name)
                .unwrap();
            let tdef = function_result.tref.resolve(spec);
            let wasi_value = state.decode_to_wasi_value(
                self.z3_ctx,
                spec,
                &types,
                &tdef,
                &ParamDecl::Node(result_value.clone()),
                &model,
            );
            let result_resource_idx = results.get(result_idx).unwrap().unwrap();
            let resource = env.resources.get_mut(result_resource_idx).unwrap();
            let idx = env
                .reverse_resource_index
                .get_mut(&tdef.name)
                .unwrap()
                .remove(&resource.state)
                .unwrap();

            env.reverse_resource_index
                .get_mut(&tdef.name)
                .unwrap()
                .insert(wasi_value.clone(), idx);
            resource.state = wasi_value;

            clauses.push(result._eq(&result_value).not());
        }

        solver.push();
        solver.assert(&Bool::or(self.z3_ctx, clauses.as_slice()));

        match solver.check() {
            | z3::SatResult::Unsat => (),
            | _ => return Err(err!("more than one solution for output contract")),
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Segment<'a> {
    Separator,
    Component(&'a str),
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PreopenFs {
    root: Directory,
}

impl PreopenFs {
    fn new(path: &Path) -> Result<Self, eyre::Error> {
        Ok(Self {
            root: Directory::ingest(path)?,
        })
    }
}

#[derive(Debug)]
struct PreopenFsEncoding<'ctx> {
    root: DirectoryEncoding<'ctx>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum File {
    Directory(Directory),
    RegularFile(RegularFile),
}

impl File {
    fn declare<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        types: &StateTypes<'ctx>,
    ) -> FileEncoding<'ctx> {
        match self {
            | File::Directory(directory) => {
                let dir = directory.declare(ctx, types);

                FileEncoding::Directory(dir)
            },
            | File::RegularFile(regular_file) => {
                FileEncoding::RegularFile(regular_file.declare(ctx, types))
            },
        }
    }

    fn ingest(path: &Path) -> Result<Self, eyre::Error> {
        let metadata = fs::metadata(path)?;

        if metadata.file_type().is_dir() {
            Ok(Self::Directory(Directory::ingest(path)?))
        } else if metadata.file_type().is_file() {
            Ok(Self::RegularFile(RegularFile::ingest(path)?))
        } else {
            unimplemented!("unsupported file type")
        }
    }
}

#[derive(Debug)]
enum FileEncoding<'ctx> {
    Directory(DirectoryEncoding<'ctx>),
    RegularFile(RegularFileEncoding<'ctx>),
}

impl<'ctx> FileEncoding<'ctx> {
    fn node(&self) -> &Dynamic {
        match self {
            | FileEncoding::Directory(d) => &d.node,
            | FileEncoding::RegularFile(f) => &f.node,
        }
    }

    fn as_ref(&self) -> FileEncodingRef<'ctx, '_> {
        match self {
            | FileEncoding::Directory(d) => FileEncodingRef::Directory(d),
            | FileEncoding::RegularFile(f) => FileEncodingRef::RegularFile(f),
        }
    }
}

#[derive(Debug)]
enum FileEncodingRef<'ctx, 'a> {
    Directory(&'a DirectoryEncoding<'ctx>),
    RegularFile(&'a RegularFileEncoding<'ctx>),
}

impl<'ctx, 'a> FileEncodingRef<'ctx, 'a> {
    fn directory(&self) -> Option<&'a DirectoryEncoding<'ctx>> {
        match self {
            | &FileEncodingRef::Directory(d) => Some(d),
            | FileEncodingRef::RegularFile(_) => None,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Directory {
    children: IndexSpace<String, File>,
}

impl Directory {
    fn declare<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        types: &StateTypes<'ctx>,
    ) -> DirectoryEncoding<'ctx> {
        let node = Dynamic::fresh_const(ctx, "file--", &types.file.sort);
        let mut children = BTreeMap::new();

        for (name, child) in self.children.iter() {
            let child = child.declare(ctx, types);

            children.insert(name.to_owned(), child);
        }

        DirectoryEncoding { node, children }
    }

    fn ingest(path: &Path) -> Result<Self, eyre::Error> {
        let mut paths: Vec<PathBuf> = Default::default();

        for entry in fs::read_dir(path).wrap_err("failed to read dir")? {
            let entry = entry?;

            paths.push(entry.path());
        }

        paths.sort();

        let mut children = IndexSpace::new();

        for path in &paths {
            let file = File::ingest(&path)?;

            children.push(
                String::from_utf8(path.file_name().unwrap().as_encoded_bytes().to_vec()).unwrap(),
                file,
            );
        }

        Ok(Self { children })
    }
}

#[derive(Default, Debug)]
struct ToSolves<'ctx> {
    results: BTreeMap<String, Dynamic<'ctx>>,
}

#[derive(Debug)]
struct DirectoryEncoding<'ctx> {
    node:     Dynamic<'ctx>,
    children: BTreeMap<String, FileEncoding<'ctx>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct RegularFile {}

impl RegularFile {
    fn declare<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        types: &StateTypes<'ctx>,
    ) -> RegularFileEncoding<'ctx> {
        let node = Dynamic::fresh_const(ctx, "file--", &types.file.sort);

        RegularFileEncoding { node }
    }

    fn ingest(_path: &Path) -> Result<Self, io::Error> {
        Ok(Self {})
    }
}

#[derive(Debug)]
struct RegularFileEncoding<'ctx> {
    node: Dynamic<'ctx>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Type {
    Wasi(TypeDef),
    Wazzi(WazziType),
}

impl Type {
    fn wasi(&self) -> Option<&TypeDef> {
        match self {
            | Self::Wasi(tdef) => Some(tdef),
            | _ => None,
        }
    }

    fn unwrap_ast_as_int<'ctx>(
        &self,
        types: &'ctx StateTypes<'ctx>,
        ast: &Dynamic<'ctx>,
    ) -> Int<'ctx> {
        match &self {
            | Type::Wasi(tdef) => match &tdef.wasi {
                | WasiType::S64 | WasiType::U8 | WasiType::U16 | WasiType::U32 | WasiType::U64 => {
                    types.resources.get(&tdef.name).unwrap().variants[0].accessors[0]
                        .apply(&[ast])
                        .as_int()
                        .unwrap()
                },
                | _ => panic!(),
            },
            | Type::Wazzi(WazziType::Int) => ast.as_int().unwrap(),
            | _ => panic!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum WazziType {
    Int,
    Bool,
    Lambda(Box<LambdaType>),
    List(Box<Type>),
    String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct LambdaType {
    range: Type,
}
