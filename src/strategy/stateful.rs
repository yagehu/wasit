use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{self, Read},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
};

use arbitrary::Unstructured;
use eyre::{eyre as err, Context};
use idxspace::IndexSpace;
use itertools::Itertools;
use petgraph::{data::DataMap as _, graph::DiGraph, visit::IntoNeighborsDirected};
use z3::{
    ast::{lambda_const, Ast, Bool, Dynamic, Int, Seq},
    FuncDecl,
};

use super::CallStrategy;
use crate::{
    resource::HighLevelValue,
    spec::{
        witx::slang::{self, NoNonExistentDirBacktrack, Term},
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
                let parent_resource_idx = ResourceIdx(parent_value.u64().unwrap().try_into().unwrap());
                let parent_node_idx = *self.fds_idxs.get(&parent_resource_idx).unwrap();

                self.fds_graph.add_edge(node_idx, parent_node_idx, path);
            }
        }

        self.resources.insert(idx, value);
    }

    fn declare2<'ctx>(&self, decls: &'ctx StateDecls<'ctx>) -> StateDecls2<'ctx> {
        let fds_graph_rev = petgraph::visit::Reversed(&self.fds_graph);
        let mut topo = petgraph::visit::Topo::new(&fds_graph_rev);
        let mut fd_file = Vec::new();
        let mut fd_file_vec = Vec::new();
        let mut fd_file_map = HashMap::new();
        let mut fd_dir_map = HashMap::new();

        for (&idx, preopen) in decls.preopens.iter() {
            let fd = decls.resources.get(&idx).unwrap();

            fd_file_vec.push(fd.clone());
            fd_file_map.insert(fd.clone(), fd_file_vec.len() - 1);
            fd_file.push(preopen.root.node.clone());
            fd_dir_map.insert(fd, preopen.root.clone());
        }

        while let Some(node_idx) = topo.next(fds_graph_rev) {
            let fd_resource_idx = *fds_graph_rev.node_weight(node_idx).unwrap();
            let fd = decls.resources.get(&fd_resource_idx).unwrap();
            let dir = match fd_dir_map.get(fd) {
                | Some(dir) => dir.clone(),
                | None => continue,
            };

            for child_node_idx in fds_graph_rev.neighbors_directed(node_idx, petgraph::Direction::Outgoing) {
                // let mut curr = FileEncodingRef::Directory(&dir);
                // let mut prevs = Vec::new();
                let child_fd_resource_idx = *fds_graph_rev.node_weight(child_node_idx).unwrap();
                let edge_idx = self.fds_graph.find_edge(child_node_idx, node_idx).unwrap();
                // let path = self.fds_graph.edge_weight(edge_idx).unwrap();

                // for component in PathBuf::from(path).components() {
                //     let component = component.as_os_str().to_str().unwrap();

                //     match component {
                //         | "." => (),
                //         | ".." => curr = prevs.pop().unwrap(),
                //         | component => {
                //             let child = curr
                //                 .directory()
                //                 .expect("not a directory")
                //                 .children
                //                 .get(component)
                //                 .expect("not such child");

                //             prevs.push(curr);
                //             curr = match child {
                //                 | FileEncoding::Directory(d) => FileEncodingRef::Directory(d),
                //                 | FileEncoding::RegularFile(f) => FileEncodingRef::RegularFile(f),
                //                 | FileEncoding::Symlink(l) => FileEncodingRef::Symlink(l),
                //             };
                //         },
                //     }
                // }

                let child_fd = decls.resources.get(&child_fd_resource_idx).unwrap();

                fd_file_vec.push(child_fd.clone());
                fd_file_map.insert(child_fd.clone(), fd_file_vec.len() - 1);
                // fd_file.push(curr.node().clone());

                // if let FileEncodingRef::Directory(d) = curr {
                //     fd_dir_map.insert(child_fd, d.clone());
                // }
            }
        }

        let mut children = Vec::new();
        let mut children_vec = Vec::new();

        // Children relation. Maps any file (directory) to its children.
        {
            let mut children_entries = Vec::new();

            for (_idx, preopen) in decls.preopens.iter() {
                let mut dirs = vec![&preopen.root];

                while let Some(dir) = dirs.pop() {
                    for (name, child) in &dir.children {
                        children_entries.push((dir.node.clone(), name, child.node().clone()));

                        match child {
                            | FileEncoding::Directory(d) => dirs.push(d),
                            | _ => continue,
                        }
                    }
                }
            }

            for (parent, filename, child) in children_entries {
                let children = match children_vec.iter().enumerate().find(|&(_i, p)| p == &parent) {
                    | Some((i, _p)) => children.get_mut(i).unwrap(),
                    | None => {
                        children_vec.push(parent.clone());
                        children.push(BTreeMap::new());
                        children.last_mut().unwrap()
                    },
                };

                children.insert(filename.to_owned(), child.clone());
            }
        }

        StateDecls2 {
            fd_file,
            fd_file_vec,
            fd_file_map,
            children,
            children_vec,
        }
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
                        .resource_wrappers
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
                    | Term::True => (),
                    | Term::String(_) => (),
                    | Term::Not(t) => scan_primed_in_output_contract(ctx, types, spec, function, &t.term, to_solves),
                    | Term::And(t) => {
                        for clause in &t.clauses {
                            scan_primed_in_output_contract(ctx, types, spec, function, clause, to_solves);
                        }
                    },
                    | Term::Or(t) => {
                        for clause in &t.clauses {
                            scan_primed_in_output_contract(ctx, types, spec, function, clause, to_solves);
                        }
                    },
                    | Term::RecordField(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.target, to_solves)
                    },
                    | Term::Param(t) => {
                        if let Some(name) = t.name.strip_suffix('\'') {
                            let function_param = function.params.iter().find(|param| param.name == name).unwrap();
                            let tdef = function_param.tref.resolve(spec);
                            let datatype = types.resources.get(&tdef.name).unwrap();

                            to_solves.params.insert(
                                name.to_string(),
                                Dynamic::new_const(ctx, format!("param--{}", t.name), &datatype.sort),
                            );
                        }
                    },
                    | Term::Result(t) => {
                        if let Some(name) = t.name.strip_suffix('\'') {
                            let function_result = function.results.iter().find(|result| result.name == name).unwrap();
                            let tdef = function_result.tref.resolve(spec);
                            let datatype = types.resources.get(&tdef.name).unwrap();

                            to_solves.results.insert(
                                t.name.strip_suffix('\'').unwrap().to_string(),
                                Dynamic::new_const(ctx, format!("result--{}", t.name), &datatype.sort),
                            );
                        }
                    },
                    | Term::ResourceId(_) => (),
                    | Term::FlagsGet(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.target, to_solves)
                    },
                    | Term::ListLen(_t) => todo!(),
                    | Term::IntWrap(t) => scan_primed_in_output_contract(ctx, types, spec, function, &t.op, to_solves),
                    | Term::IntConst(_t) => (),
                    | Term::IntAdd(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.lhs, to_solves);
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.rhs, to_solves);
                    },
                    | Term::IntGt(_t) => todo!(),
                    | Term::IntLe(_t) => todo!(),
                    | Term::U64Const(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.term, to_solves)
                    },
                    | Term::StrAt(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.lhs, to_solves);
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.rhs, to_solves);
                    },
                    | Term::ValueEq(t) => {
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.lhs, to_solves);
                        scan_primed_in_output_contract(ctx, types, spec, function, &t.rhs, to_solves);
                    },
                    | Term::VariantConst(t) => {
                        if let Some(payload) = &t.payload {
                            scan_primed_in_output_contract(ctx, types, spec, function, payload, to_solves);
                        }
                    },
                    | Term::FsFileSizeGet(_t) => (),
                    | Term::FsFileTypeGet(_t) => (),
                    | Term::FsFileTypeGetl(_t) => (),
                    | Term::NoNonExistentDirBacktrack(_t) => todo!(),
                }
            }

            scan_primed_in_output_contract(ctx, types, spec, function, term, &mut to_solves);
        }

        let params = function
            .params
            .iter()
            .map(|param| (&param.name, param.tref.resolve(spec)))
            .map(|(param_name, tdef)| match tdef.name.as_str() {
                | "path" => {
                    let len = match &mut aop {
                        | ArbitraryOrPresolved::Arbitrary(u) => u.choose(&[0, 8]).unwrap() + 1,
                        | ArbitraryOrPresolved::Presolved(lens) => *lens.get(param_name).unwrap(),
                    };
                    let len = if len == 2 { 1 } else { len };

                    (
                        param_name.to_owned(),
                        ParamDecl::Path {
                            segments: (0..len)
                                .map(|_i| Dynamic::fresh_const(ctx, "param-segment--", &types.segment.sort))
                                .collect_vec(),
                        },
                    )
                },
                | _ => {
                    let datatype = types.resource_wrappers.get(&tdef.name).expect(&tdef.name);

                    (
                        param_name.to_owned(),
                        ParamDecl::Node(Dynamic::fresh_const(ctx, "param--", &datatype.sort)),
                    )
                },
            })
            .collect();

        StateDecls {
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
        decls2: &'ctx StateDecls2<'ctx>,
        spec: &Spec,
        function: &Function,
        params: Option<&[HighLevelValue]>,
        results: Option<&[WasiValue]>,
        contract: Option<&Term>,
    ) -> Bool<'ctx> {
        let mut clauses = Vec::new();
        let mut all_files = Vec::new();

        for (&_resource_idx, preopen) in decls.preopens.iter() {
            all_files.push(&preopen.root.node);

            let mut dirs = vec![&preopen.root];

            while let Some(dir) = dirs.pop() {
                for (_filename, child) in &dir.children {
                    all_files.push(child.node());

                    match child {
                        | FileEncoding::Directory(d) => dirs.push(&d),
                        | _ => (),
                    }
                }
            }
        }

        {
            let mut all_dirs = decls.preopens.values().map(|preopen| &preopen.root.node).collect_vec();
            let mut all_files = vec![];
            let mut all_symlinks = vec![];

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
                            | FileEncoding::Symlink(l) => all_symlinks.push(&l.node),
                        }
                    }
                }
            }

            clauses.push(Bool::and(
                ctx,
                all_dirs
                    .iter()
                    .map(|&dir| types.file.variants[0].tester.apply(&[dir]).as_bool().unwrap())
                    .collect_vec()
                    .as_slice(),
            ));
            clauses.push(Bool::and(
                ctx,
                all_files
                    .into_iter()
                    .map(|dir| types.file.variants[1].tester.apply(&[dir]).as_bool().unwrap())
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
                        | FileEncoding::Symlink(l) => {
                            clauses.push(
                                types.file.variants[2].accessors[0]
                                    .apply(&[&l.node])
                                    .as_int()
                                    .unwrap()
                                    ._eq(&Int::from_u64(ctx, idx)),
                            );
                            idx += 1;
                        },
                    }
                }
            }
        }

        // Constrain all the resource values.
        {
            clauses.push(Bool::and(
                ctx,
                decls
                    .params
                    .iter()
                    .filter_map(|(param_name, param_node)| {
                        let param = function.params.iter().find(|p| &p.name == param_name).unwrap();
                        let tdef = param.tref.resolve(spec);

                        if tdef.state.is_some() {
                            let idxs = env.resources_by_types.get(&tdef.name).cloned().unwrap_or_default();

                            Some(Bool::or(
                                ctx,
                                idxs.iter()
                                    .map(|&idx| {
                                        let resource = env.resources.get(idx).unwrap();

                                        types.encode_wasi_value_decl(
                                            ctx,
                                            spec,
                                            param_node,
                                            tdef,
                                            &resource.state,
                                            Some(idx),
                                        )
                                    })
                                    .collect_vec()
                                    .as_slice(),
                            ))
                        } else {
                            if let ParamDecl::Node(node) = param_node {
                                let datatype = types.resource_wrappers.get(&tdef.name).unwrap();

                                Some(
                                    datatype.variants[0].accessors[0]
                                        .apply(&[node])
                                        ._eq(&Dynamic::from_ast(&Int::from_i64(ctx, -1))),
                                )
                            } else {
                                None
                            }
                        }
                    })
                    .collect_vec()
                    .as_slice(),
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
                    .enumerate()
                    .map(|(_i, segment)| {
                        Bool::and(
                            ctx,
                            &[&component
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
                                                    ._eq(&z3::ast::String::from_str(ctx, "b").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "c").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "d").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "e").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "f").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "g").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                                                &component.accessors[0]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "..").unwrap()),
                                            ],
                                        ),
                                    ],
                                ))],
                        )
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
                        // separator
                        //     .tester
                        //     .apply(&[segment])
                        //     .as_bool()
                        //     .unwrap()
                        //     .implies(&separator.tester.apply(&[prev]).as_bool().unwrap().not()),
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

        // Final segment should not be a separator.
        for path in paths.iter() {
            clauses.push(Bool::and(
                ctx,
                &[separator
                    .tester
                    .apply(&[path.first().unwrap()])
                    .as_bool()
                    .unwrap()
                    .not()],
            ));
            clauses.push(Bool::and(
                ctx,
                &[separator.tester.apply(&[path.last().unwrap()]).as_bool().unwrap().not()],
            ));
        }

        // Constrain non-resource param values.
        if let Some(params) = params {
            for (function_param, value) in function.params.iter().zip(params.iter()) {
                let param_decl = decls.params.get(&function_param.name).unwrap();
                let tdef = function_param.tref.resolve(spec);
                let value = env.resolve_value(value);

                if tdef.state.is_some() {
                    continue;
                }

                clauses.push(types.encode_wasi_value_decl(ctx, spec, param_decl, tdef, &value, None));
            }
        }

        // Constrain param resources.
        for (param_name, param_node) in decls.params.iter() {
            let param = function.params.iter().find(|param| &param.name == param_name).unwrap();
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
                    decls2,
                    term,
                    function,
                    params,
                    results,
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
        decls2: &'ctx StateDecls2<'ctx>,
        term: &Term,
        function: &Function,
        params: Option<&[HighLevelValue]>,
        results: Option<&[WasiValue]>,
    ) -> (Dynamic<'ctx>, Type) {
        match term {
            | Term::Foldl(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.target, function, params, results,
                );
                let (acc, acc_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.acc, function, params, results,
                );
                let (func, _func_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.func, function, params, results,
                );

                (
                    types.resources.get(&target_type.wasi().unwrap().name).unwrap().variants[0].accessors[0]
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
                                | slang::WazziType::Bool => (z3::Sort::bool(ctx), Type::Wazzi(WazziType::Bool)),
                                | slang::WazziType::Int => (z3::Sort::int(ctx), Type::Wazzi(WazziType::Int)),
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
                    ctx, env, &eval_ctx, spec, types, decls, decls2, &t.body, function, params, results,
                );

                (
                    Dynamic::from_ast(&lambda_const(ctx, bounds.as_slice(), &body)),
                    Type::Wazzi(WazziType::Lambda(Box::new(LambdaType { range: r#type }))),
                )
            },
            | Term::Map(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.target, function, params, results,
                );
                let (func, func_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.func, function, params, results,
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
                    types.resources.get(&target_type.wasi().unwrap().name).unwrap().variants[0].accessors[0]
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.term, function, params, results,
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
                                ctx, env, eval_ctx, spec, types, decls, decls2, clause, function, params, results,
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
                                ctx, env, eval_ctx, spec, types, decls, decls2, clause, function, params, results,
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.target, function, params, results,
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
                if t.name.ends_with('\'') {
                    let function_param = function
                        .params
                        .iter()
                        .find(|param| param.name == t.name.strip_suffix('\'').unwrap())
                        .unwrap();
                    let tdef = function_param.tref.resolve(spec);

                    (
                        decls
                            .to_solves
                            .params
                            .get(t.name.strip_suffix('\'').unwrap())
                            .unwrap()
                            .clone(),
                        Type::Wasi(tdef.to_owned()),
                    )
                } else {
                    let tdef = function
                        .params
                        .iter()
                        .find(|p| p.name == t.name)
                        .unwrap()
                        .tref
                        .resolve(spec);
                    let param = decls.params.get(&t.name).expect(&format!("{}", t.name));
                    let node = match param {
                        | ParamDecl::Node(node) => {
                            types.resource_wrappers.get(&tdef.name).unwrap().variants[0].accessors[1].apply(&[node])
                        },
                        | ParamDecl::Path { segments } => {
                            let path_type = types.resources.get("path").unwrap();
                            let segments = segments.iter().map(|segment| Seq::unit(ctx, segment)).collect_vec();
                            let segments = Seq::concat(ctx, &segments);

                            path_type.variants[0].constructor.apply(&[&segments])
                        },
                    };

                    (node, Type::Wasi(tdef.to_owned()))
                }
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
                    let (idx, result) = function
                        .results
                        .iter()
                        .enumerate()
                        .find(|(_i, r)| r.name == t.name)
                        .unwrap();
                    let result_value = results.unwrap().get(idx).unwrap();

                    (
                        types.encode_wasi_value(ctx, spec, result.tref.resolve(spec), result_value),
                        Type::Wasi(result.tref.resolve(spec).clone()),
                    )
                }
            },
            | Term::ResourceId(name) => {
                let (_param_idx, function_param) = function
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, param)| &param.name == name)
                    .unwrap();
                let datatype = types
                    .resource_wrappers
                    .get(&function_param.tref.resolve(spec).name)
                    .unwrap();
                let param_decl = decls.params.get(&function_param.name).unwrap();

                match param_decl {
                    | ParamDecl::Node(node) => {
                        let node = datatype.variants[0].accessors[0].apply(&[node]);
                        (node, Type::Wazzi(WazziType::Int))
                    },
                    | ParamDecl::Path { .. } => unimplemented!(),
                }

                // let resource_idx = params.unwrap().get(param_idx).unwrap().as_resource().unwrap();

                // (
                //     Dynamic::from_ast(&Int::from_u64(ctx, resource_idx.0 as u64)),
                //     Type::Wazzi(WazziType::Int),
                // )
            },
            | Term::FlagsGet(t) => {
                let (target, target_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.target, function, params, results,
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.op, function, params, results,
                );

                (
                    Dynamic::from_ast(&op_type.unwrap_ast_as_int(types, &op)),
                    Type::Wazzi(WazziType::Int),
                )
            },
            | Term::IntAdd(t) => {
                let (lhs, lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.lhs, function, params, results,
                );
                let (rhs, rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.rhs, function, params, results,
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
                let (lhs, lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.lhs, function, params, results,
                );
                let (rhs, rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.rhs, function, params, results,
                );

                (
                    Dynamic::from_ast(
                        &lhs_type
                            .unwrap_ast_as_int(types, &lhs)
                            .gt(&rhs_type.unwrap_ast_as_int(types, &rhs)),
                    ),
                    Type::Wazzi(WazziType::Bool),
                )
            },
            | Term::IntLe(t) => {
                let (lhs, lhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.lhs, function, params, results,
                );
                let (rhs, rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.rhs, function, params, results,
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.op, function, params, results,
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.term, function, params, results,
                );

                let datatype = types.resources.get("u64").unwrap();

                (
                    datatype.variants[0].constructor.apply(&[&value]),
                    Type::Wasi(spec.types.get_by_key("u64").unwrap().clone()),
                )
            },
            | Term::StrAt(t) => {
                let (mut s, s_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.lhs, function, params, results,
                );
                let (i, _type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.rhs, function, params, results,
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
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.lhs, function, params, results,
                );
                let (rhs, _rhs_type) = self.term_to_z3_ast(
                    ctx, env, eval_ctx, spec, types, decls, decls2, &t.rhs, function, params, results,
                );

                (Dynamic::from_ast(&lhs._eq(&rhs)), Type::Wazzi(WazziType::Bool))
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
                            decls2,
                            payload_term,
                            function,
                            params,
                            results,
                        );

                        vec![payload]
                    },
                    | None => vec![],
                };

                (
                    datatype.variants[i]
                        .constructor
                        .apply(payload.iter().map(|p| p as &dyn z3::ast::Ast).collect_vec().as_slice()),
                    Type::Wasi(variant_tdef.to_owned()),
                )
            },
            | Term::FsFileSizeGet(t) => {
                let path = match &t.path {
                    | Term::String(string) => string,
                    | _ => unimplemented!(),
                };
                let (fd_param_idx, _fd_function_param) = function
                    .params
                    .iter()
                    .enumerate()
                    .find(|(_, param)| param.name == t.fd)
                    .unwrap();
                let fd_resource_idx = params.unwrap().get(fd_param_idx).unwrap().as_resource();
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
                let mut paths = if path.is_empty() { vec![] } else { vec![path.clone()] };

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

                for path in paths.iter().rev() {
                    let path = Path::new(path);

                    for component in path.components() {
                        let filename = String::from_utf8(component.as_os_str().as_encoded_bytes().to_vec()).unwrap();
                        let f = files.last().unwrap();

                        match filename.as_str() {
                            | ".." => {
                                files.pop();
                            },
                            | "." => (),
                            | filename => match f {
                                | FileEncodingRef::Directory(d) => files.push(
                                    d.children
                                        .get(filename)
                                        .expect(&format!("{filename} {:#?}", d.children))
                                        .as_ref(),
                                ),
                                | _ => unreachable!(),
                            },
                        }
                    }
                }

                let size = match files.last().unwrap() {
                    | FileEncodingRef::Directory(_d) => unimplemented!(),
                    | FileEncodingRef::RegularFile(f) => f.size,
                    | FileEncodingRef::Symlink(_l) => unimplemented!(),
                };

                (
                    Dynamic::from_ast(&Int::from_u64(ctx, size)),
                    Type::Wazzi(WazziType::Int),
                )
            },
            | Term::FsFileTypeGetl(t) => {
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

                match params.unwrap().get(fd_param_idx) {
                    | None => {
                        // This is a part of an input contract.
                        // Try to encode this term.

                        // Assume just one preopen to keep things simple.
                        let mut files = vec![FileEncodingRef::Directory(
                            &decls.preopens.iter().next().unwrap().1.root,
                        )];

                        loop {
                            while let Some(file) = files.pop() {
                                // For each file, try to encode.

                                if let FileEncodingRef::Directory(d) = file {
                                    for (_name, child) in &d.children {
                                        files.push(child.as_ref());
                                    }
                                }
                            }
                        }

                        // TODO
                    },
                    | Some(_) => {},
                }

                let fd_resource_idx = params.unwrap().get(fd_param_idx).unwrap().as_resource();
                let path_value = env.resolve_value(params.unwrap().get(path_param_idx).unwrap());
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
                let mut paths = vec![String::from_utf8(path_value.string().unwrap().to_vec()).unwrap()];

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

                for path in paths.iter().rev() {
                    let mut path_stack = vec![(
                        Path::new(path)
                            .components()
                            .map(|c| String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec()).unwrap())
                            .collect_vec(),
                        0,
                    )];

                    while let Some((components, i)) = path_stack.last_mut() {
                        let component = match components.get(*i) {
                            | None => {
                                path_stack.pop();
                                continue;
                            },
                            | Some(component) => component,
                        };
                        let f = files.last().unwrap();

                        *i += 1;

                        match component.as_str() {
                            | ".." => {
                                files.pop();
                            },
                            | "." => (),
                            | filename => match f {
                                | FileEncodingRef::Directory(d) => files.push(
                                    d.children
                                        .get(filename)
                                        .expect(&format!("{filename} {:#?}", d.children))
                                        .as_ref(),
                                ),
                                | FileEncodingRef::Symlink(l) => {
                                    *i -= 1;
                                    path_stack.push((
                                        Path::new(&l.content)
                                            .components()
                                            .map(|c| {
                                                String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec()).unwrap()
                                            })
                                            .collect_vec(),
                                        0,
                                    ));
                                    files.pop();
                                },
                                | _ => unreachable!("{}, {:#?}", filename, f),
                            },
                        }
                    }
                }

                let filetype_tdef = spec.types.get_by_key("filetype").unwrap();
                let case_idx = loop {
                    match files.pop().unwrap() {
                        | FileEncodingRef::Symlink(l) => {
                            let mut path_stack = vec![(
                                Path::new(&l.content)
                                    .components()
                                    .map(|c| String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec()).unwrap())
                                    .collect_vec(),
                                0,
                            )];

                            while let Some((components, i)) = path_stack.last_mut() {
                                let component = match components.get(*i) {
                                    | None => {
                                        path_stack.pop();
                                        continue;
                                    },
                                    | Some(component) => component,
                                };
                                let f = files.last().unwrap();

                                *i += 1;

                                match component.as_str() {
                                    | ".." => {
                                        files.pop();
                                    },
                                    | "." => (),
                                    | filename => match f {
                                        | FileEncodingRef::Directory(d) => files.push(
                                            d.children
                                                .get(filename)
                                                .expect(&format!("{filename} {:#?}", d.children))
                                                .as_ref(),
                                        ),
                                        | FileEncodingRef::Symlink(l) => {
                                            *i -= 1;
                                            path_stack.push((
                                                Path::new(&l.content)
                                                    .components()
                                                    .map(|c| {
                                                        String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec())
                                                            .unwrap()
                                                    })
                                                    .collect_vec(),
                                                0,
                                            ));
                                            files.pop();
                                        },
                                        | _ => unreachable!("{}, {:#?}", filename, f),
                                    },
                                }
                            }
                        },
                        | FileEncodingRef::Directory(_d) => {
                            break filetype_tdef
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
                            break filetype_tdef
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
                    }
                };

                (
                    types.resources.get("filetype").unwrap().variants[case_idx]
                        .constructor
                        .apply(&[]),
                    Type::Wasi(filetype_tdef.to_owned()),
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

                match params.unwrap().get(fd_param_idx) {
                    | None => {
                        // This is a part of an input contract.
                        // Try to encode this term.

                        // Assume just one preopen to keep things simple.
                        let mut files = vec![FileEncodingRef::Directory(
                            &decls.preopens.iter().next().unwrap().1.root,
                        )];

                        loop {
                            while let Some(file) = files.pop() {
                                // For each file, try to encode.

                                if let FileEncodingRef::Directory(d) = file {
                                    for (_name, child) in &d.children {
                                        files.push(child.as_ref());
                                    }
                                }
                            }
                        }

                        // TODO
                    },
                    | Some(_) => {},
                }

                let fd_resource_idx = params.unwrap().get(fd_param_idx).unwrap().as_resource();
                let path_value = env.resolve_value(params.unwrap().get(path_param_idx).unwrap());
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
                let mut paths = vec![String::from_utf8(path_value.string().unwrap().to_vec()).unwrap()];

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

                for path in paths.iter().rev() {
                    let mut path_stack = vec![(
                        Path::new(path)
                            .components()
                            .map(|c| String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec()).unwrap())
                            .collect_vec(),
                        0,
                    )];

                    while let Some((components, i)) = path_stack.last_mut() {
                        let component = match components.get(*i) {
                            | None => {
                                path_stack.pop();
                                continue;
                            },
                            | Some(component) => component,
                        };
                        let f = files.last().unwrap();

                        *i += 1;

                        match component.as_str() {
                            | ".." => {
                                files.pop();
                            },
                            | "." => (),
                            | filename => match f {
                                | FileEncodingRef::Directory(d) => files.push(
                                    d.children
                                        .get(filename)
                                        .expect(&format!("{filename} {:#?} {:?}", d.children, thread::current().name()))
                                        .as_ref(),
                                ),
                                | FileEncodingRef::Symlink(l) => {
                                    *i -= 1;
                                    path_stack.push((
                                        Path::new(&l.content)
                                            .components()
                                            .map(|c| {
                                                String::from_utf8(c.as_os_str().as_encoded_bytes().to_vec()).unwrap()
                                            })
                                            .collect_vec(),
                                        0,
                                    ));
                                    files.pop();
                                },
                                | _ => unreachable!("{}, {:#?}, {:?}", filename, f, thread::current().name()),
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
                    | FileEncodingRef::Symlink(_l) => {
                        filetype_tdef
                            .wasi
                            .variant()
                            .unwrap()
                            .cases
                            .iter()
                            .enumerate()
                            .find(|(_i, case)| case.name == "symbolic_link")
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
            | Term::NoNonExistentDirBacktrack(t) => (
                no_nonexistent_dir_backtrack(ctx, types, decls, decls2, t),
                Type::Wazzi(WazziType::Bool),
            ),
        }
    }

    fn decode_to_wasi_value<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        types: &'ctx StateTypes<'ctx>,
        tdef: &TypeDef,
        decl: &ParamDecl,
        model: &z3::Model<'ctx>,
    ) -> (WasiValue, Option<ResourceIdx>) {
        match decl {
            | ParamDecl::Node(node) => {
                let datatype = types.resource_wrappers.get(&tdef.name).unwrap();
                let resource_idx = if tdef.state.is_some() {
                    Some(ResourceIdx(
                        model
                            .eval(
                                &datatype.variants[0].accessors[0].apply(&[node]).as_int().unwrap(),
                                true,
                            )
                            .unwrap()
                            .as_u64()
                            .unwrap() as usize,
                    ))
                } else {
                    None
                };
                let node = datatype.variants[0].accessors[1].apply(&[node]);

                (
                    self.decode_to_wasi_value_inner(ctx, spec, types, tdef, &ParamDecl::Node(node), model),
                    resource_idx,
                )
            },
            | ParamDecl::Path { .. } => (
                self.decode_to_wasi_value_inner(ctx, spec, types, tdef, &decl, model),
                None,
            ),
        }
    }

    fn decode_to_wasi_value_inner<'ctx>(
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
                model
                    .eval(
                        &datatype.variants[0].accessors[0]
                            .apply(&[decl.node()])
                            .as_int()
                            .unwrap(),
                        true,
                    )
                    .unwrap()
                    .as_i64()
                    .unwrap(),
            ),
            | WasiType::U8 => WasiValue::U8(
                model
                    .eval(
                        &datatype.variants[0].accessors[0]
                            .apply(&[decl.node()])
                            .as_int()
                            .unwrap(),
                        true,
                    )
                    .unwrap()
                    .as_i64()
                    .expect(&format!("{:#?}", decl)) as u8,
            ),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => {
                let i = model
                    .eval(&datatype.variants[0].accessors[0].apply(&[decl.node()]), true)
                    .unwrap()
                    .as_int()
                    .unwrap();

                WasiValue::U32(i.as_i64().unwrap() as u32)
            },
            | WasiType::U64 => WasiValue::U64(
                model
                    .eval(&datatype.variants[0].accessors[0].apply(&[decl.node()]), true)
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
                            .eval(&datatype.variants[0].accessors[i].apply(&[decl.node()]), true)
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
                    | Some(payload) => Some(self.decode_to_wasi_value_inner(
                        ctx,
                        spec,
                        types,
                        payload.tref().unwrap().resolve(spec),
                        &ParamDecl::Node(datatype.variants[case_idx].accessors[0].apply(&[decl.node()])),
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
                        self.decode_to_wasi_value_inner(
                            ctx,
                            spec,
                            types,
                            member.tref.resolve(spec),
                            &ParamDecl::Node(datatype.variants[0].accessors[i].apply(&[decl.node()])),
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
                                &types.resources.get("path").unwrap().variants[0].accessors[0].apply(&[node]),
                                true,
                            )
                            .unwrap()
                            .as_seq()
                            .unwrap();
                        let mut s = String::new();

                        for i in 0..model.eval(&seq.length(), true).unwrap().as_u64().unwrap() {
                            let seg = seq.nth(&Int::from_u64(ctx, i));

                            if model
                                .eval(&types.segment.variants[0].tester.apply(&[&seg]), true)
                                .unwrap()
                                .as_bool()
                                .unwrap()
                                .as_bool()
                                .unwrap()
                            {
                                s.push('/');
                            } else {
                                s.push_str(
                                    model
                                        .eval(
                                            &types.segment.variants[1].accessors[0]
                                                .apply(&[&seg])
                                                .as_string()
                                                .unwrap(),
                                            true,
                                        )
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
                            &types.segment.variants[0].tester.apply(&[segment]).as_bool().unwrap(),
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
                                .eval(&types.segment.variants[1].accessors[0].apply(&[segment]), true)
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
                let length = model.eval(&seq.length(), true).unwrap().as_u64().unwrap();
                let mut items = Vec::with_capacity(length as usize);

                for i in 0..length {
                    items.push(self.decode_to_wasi_value_inner(
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
                let length = model.eval(&seq.length(), true).unwrap().as_u64().unwrap();
                let mut items = Vec::with_capacity(length as usize);

                for i in 0..length {
                    items.push(self.decode_to_wasi_value_inner(
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
    resource_wrappers: BTreeMap<String, z3::DatatypeSort<'ctx>>,
    resources:         BTreeMap<String, z3::DatatypeSort<'ctx>>,
    file:              z3::DatatypeSort<'ctx>,
    segment:           z3::DatatypeSort<'ctx>,
}

impl<'ctx> StateTypes<'ctx> {
    fn new(ctx: &'ctx z3::Context, spec: &Spec) -> Self {
        let mut resources = BTreeMap::new();
        let mut resource_wrappers = BTreeMap::new();

        fn encode_type<'ctx>(
            ctx: &'ctx z3::Context,
            spec: &Spec,
            name: &str,
            tdef: &TypeDef,
            resource_wrappers: &mut BTreeMap<String, z3::DatatypeSort<'ctx>>,
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
                | WasiType::S64 | WasiType::U8 | WasiType::U16 | WasiType::U32 | WasiType::U64 | WasiType::Handle => {
                    datatype.variant(name, vec![(name, z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))])
                },
                | WasiType::Flags(flags_type) => datatype.variant(
                    name,
                    flags_type
                        .fields
                        .iter()
                        .map(|field| (field.as_str(), z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx))))
                        .collect_vec(),
                ),
                | WasiType::Variant(variant_type) => {
                    for case in &variant_type.cases {
                        let fields = match &case.payload {
                            | Some(payload) => {
                                let payload_tdef = payload.tref().unwrap().resolve(spec);

                                encode_type(
                                    ctx,
                                    spec,
                                    &payload_tdef.name,
                                    payload_tdef,
                                    resource_wrappers,
                                    resource_types,
                                );

                                vec![(
                                    "payload",
                                    z3::DatatypeAccessor::Sort(
                                        resource_types.get(&payload_tdef.name).unwrap().sort.clone(),
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
                                resource_wrappers,
                                resource_types,
                            );

                            let member_datatype = resource_types.get(&member.tref.resolve(spec).name).unwrap();
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

            let datatype = datatype.finish();
            let wrapper_datatype = z3::DatatypeBuilder::new(ctx, format!("{name}--wrapper"))
                .variant(
                    name,
                    vec![
                        ("id", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx))),
                        (name, z3::DatatypeAccessor::Sort(datatype.sort.clone())),
                    ],
                )
                .finish();

            resource_wrappers.insert(tdef.name.clone(), wrapper_datatype);
            resource_types.insert(tdef.name.clone(), datatype);
        }

        let segment_type = z3::DatatypeBuilder::new(ctx, "segment")
            .variant(
                "separator",
                vec![],
                // vec![("segment-idx", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
            )
            .variant(
                "component",
                vec![
                    // ("segment-idx", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx))),
                    ("component-str", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx))),
                ],
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

            encode_type(ctx, spec, name, tdef, &mut resource_wrappers, &mut resources);
        }

        let file = z3::DatatypeBuilder::new(ctx, "file")
            .variant(
                "directory",
                vec![("directory-id", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
            )
            .variant(
                "regular-file",
                vec![("regular-file-id", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
            )
            .variant(
                "symlink",
                vec![("symlink-id", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
            )
            .finish();

        Self {
            resource_wrappers,
            resources,
            segment: segment_type,
            file,
        }
    }

    fn encode_wasi_value(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        tdef: &TypeDef,
        value: &WasiValue,
    ) -> Dynamic<'ctx> {
        let datatype = self.resources.get(&tdef.name).unwrap();

        match (&tdef.wasi, value) {
            | (_, &WasiValue::U8(i)) => datatype.variants[0].constructor.apply(&[&Int::from_u64(ctx, i.into())]),
            | (_, &WasiValue::U16(i)) => datatype.variants[0].constructor.apply(&[&Int::from_u64(ctx, i.into())]),
            | (_, &WasiValue::U32(i)) => datatype.variants[0].constructor.apply(&[&Int::from_u64(ctx, i.into())]),
            | (_, &WasiValue::U64(i)) => datatype.variants[0].constructor.apply(&[&Int::from_u64(ctx, i)]),
            | (_, &WasiValue::S64(i)) => datatype.variants[0].constructor.apply(&[&Int::from_i64(ctx, i)]),
            | (_, &WasiValue::Handle(h)) => datatype.variants[0].constructor.apply(&[&Int::from_u64(ctx, h.into())]),
            | (WasiType::Record(record), WasiValue::Record(record_value)) => {
                let members = record
                    .members
                    .iter()
                    .zip(record_value.members.iter())
                    .map(|(member, member_value)| {
                        self.encode_wasi_value(ctx, spec, member.tref.resolve(spec), member_value)
                    })
                    .collect_vec();

                datatype.variants[0]
                    .constructor
                    .apply(members.iter().map(|x| x as &dyn Ast).collect_vec().as_slice())
            },
            | (_, WasiValue::Record(_)) => unreachable!(),
            | (WasiType::Flags(flags), WasiValue::Flags(flags_value)) => {
                let fields = flags
                    .fields
                    .iter()
                    .zip(flags_value.fields.iter())
                    .map(|(_name, &field_value)| Bool::from_bool(ctx, field_value))
                    .collect_vec();

                datatype.variants[0]
                    .constructor
                    .apply(fields.iter().map(|x| x as &dyn Ast).collect_vec().as_slice())
            },
            | (_, WasiValue::Flags(_)) => unreachable!(),
            | _ => unimplemented!(),
        }
    }

    fn encode_wasi_value_decl(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        param: &ParamDecl<'ctx>,
        tdef: &TypeDef,
        value: &WasiValue,
        resource_idx: Option<ResourceIdx>,
    ) -> Bool<'ctx> {
        match param {
            | ParamDecl::Node(node) => {
                let datatype = self.resource_wrappers.get(&tdef.name).unwrap();
                let clause = match resource_idx {
                    | Some(resource_idx) => datatype.variants[0].accessors[0]
                        .apply(&[node])
                        ._eq(&Dynamic::from_ast(&Int::from_u64(ctx, resource_idx.0 as u64))),
                    | None => datatype.variants[0].accessors[0]
                        .apply(&[node])
                        ._eq(&Dynamic::from_ast(&Int::from_i64(ctx, -1))),
                };

                Bool::and(
                    ctx,
                    &[
                        clause,
                        self.encode_wasi_value_decl_inner(
                            ctx,
                            spec,
                            &ParamDecl::Node(datatype.variants[0].accessors[1].apply(&[node])),
                            tdef,
                            value,
                        ),
                    ],
                )
            },
            | ParamDecl::Path { .. } => self.encode_wasi_value_decl_inner(ctx, spec, param, tdef, value),
        }
    }

    fn encode_wasi_value_decl_inner(
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
                        self.encode_wasi_value_decl_inner(
                            ctx,
                            spec,
                            &ParamDecl::Node(datatype.variants[0].accessors[i].apply(&[param.node()])),
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
                    last_i = i + 1;
                    i += 1;
                }

                if last_i < i {
                    segments.push(Segment::Component(&s[last_i..]));
                }

                match param {
                    | ParamDecl::Node(dynamic) => {
                        let seq = datatype.variants[0].accessors[0].apply(&[dynamic]).as_seq().unwrap();
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
                                            | Segment::Component(s) => Bool::and(
                                                ctx,
                                                &[
                                                    self.segment.variants[1]
                                                        .tester
                                                        .apply(&[&seq.nth(&Int::from_u64(ctx, i as u64))])
                                                        .as_bool()
                                                        .unwrap(),
                                                    self.segment.variants[1].accessors[0]
                                                        .apply(&[&seq.nth(&Int::from_u64(ctx, i as u64))])
                                                        .as_string()
                                                        .unwrap()
                                                        ._eq(&z3::ast::String::from_str(ctx, s).unwrap()),
                                                ],
                                            ),
                                        })
                                        .collect_vec()
                                        .as_slice(),
                                ),
                            ],
                        )
                    },
                    | ParamDecl::Path { segments: nodes } => Bool::and(
                        ctx,
                        nodes
                            .iter()
                            .zip(segments)
                            .map(|(node, segment)| match segment {
                                | Segment::Separator => {
                                    self.segment.variants[0].tester.apply(&[node]).as_bool().unwrap()
                                },
                                | Segment::Component(s) => Bool::and(
                                    ctx,
                                    &[
                                        self.segment.variants[1].tester.apply(&[node]).as_bool().unwrap(),
                                        self.segment.variants[1].accessors[0]
                                            .apply(&[node])
                                            .as_string()
                                            .unwrap()
                                            ._eq(&z3::ast::String::from_str(ctx, s).unwrap()),
                                    ],
                                ),
                            })
                            .collect_vec()
                            .as_slice(),
                    ),
                }
            },
            | (WasiType::Variant(variant), WasiValue::Variant(variant_value)) => match &variant_value.payload {
                | Some(payload) => {
                    let payload_tdef = variant.cases[variant_value.case_idx]
                        .payload
                        .as_ref()
                        .unwrap()
                        .tref()
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
                            self.encode_wasi_value_decl_inner(
                                ctx,
                                spec,
                                &ParamDecl::Node(
                                    datatype.variants[variant_value.case_idx].accessors[0].apply(&[param.node()]),
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
            },
            | (_, WasiValue::Variant(_variant_value)) => unreachable!(),
            | (WasiType::Pointer(pointer), WasiValue::Pointer(pointer_value)) => {
                let mut clauses = vec![];

                for i in 0..pointer_value.items.len() {
                    clauses.push(
                        self.encode_wasi_value_decl_inner(
                            ctx,
                            spec,
                            &ParamDecl::Node(
                                datatype.variants[0].accessors[0]
                                    .apply(&[param.node()])
                                    .as_seq()
                                    .unwrap()
                                    .nth(&Int::from_u64(ctx, i as u64)),
                            ),
                            &pointer.item.resolve(spec),
                            pointer_value.items.get(i).unwrap(),
                        ),
                    );
                }

                Bool::and(ctx, &clauses)
            },
            | (_, WasiValue::Pointer(_pointer_value)) => unreachable!(),
            | (WasiType::List(list), WasiValue::List(list_value)) => {
                let mut clauses = vec![];

                for i in 0..list_value.items.len() {
                    clauses.push(
                        self.encode_wasi_value_decl_inner(
                            ctx,
                            spec,
                            &ParamDecl::Node(
                                datatype.variants[0].accessors[0]
                                    .apply(&[param.node()])
                                    .as_seq()
                                    .unwrap()
                                    .nth(&Int::from_u64(ctx, i as u64)),
                            ),
                            &list.item.resolve(spec),
                            list_value.items.get(i).unwrap(),
                        ),
                    );
                }

                Bool::and(ctx, &clauses)
            },
            | (_, WasiValue::List(_list_value)) => unreachable!("{:#?}", _list_value),
        }
    }
}

fn no_nonexistent_dir_backtrack<'ctx>(
    ctx: &'ctx z3::Context,
    types: &'ctx StateTypes<'ctx>,
    decls: &'ctx StateDecls<'ctx>,
    decls2: &'ctx StateDecls2<'ctx>,
    t: &NoNonExistentDirBacktrack,
) -> Dynamic<'ctx> {
    let mut clauses: Vec<Bool> = Vec::new();
    let namespace = format!("nndb-{}-{}", t.fd_param, t.path_param);
    let segment_file_exists = FuncDecl::new(
        ctx,
        format!("{namespace}--segment-file-exists"),
        &[&types.segment.sort],
        &z3::Sort::bool(ctx),
    );
    let segment_file = FuncDecl::new(
        ctx,
        format!("{namespace}--segment-file"),
        &[&types.segment.sort],
        &types.file.sort,
    );
    let param_fd = decls.params.get(&t.fd_param).unwrap().node();
    let separator = types.segment.variants.first().unwrap();
    let component = types.segment.variants.get(1).unwrap();
    let segments = match &decls.params.get(&t.path_param).unwrap() {
        | ParamDecl::Node(_param_path) => panic!(),
        | ParamDecl::Path { segments } => segments,
    };
    let mut parents = HashMap::new();

    for (_preopen_resourec_idx, preopen) in decls.preopens.iter() {
        let mut dirs = vec![&preopen.root];

        while let Some(dir) = dirs.pop() {
            for (_filename, child) in dir.children.iter() {
                parents.insert(child.node(), &dir.node);

                match child {
                    | FileEncoding::Directory(d) => dirs.push(d),
                    | _ => (),
                }
            }
        }
    }

    for (i, segment) in segments.iter().enumerate() {
        for j in 0..i {
            for (_idx, preopen) in decls.preopens.iter() {
                let frame = match i {
                    | 0 => Bool::or(
                        ctx,
                        decls2
                            .fd_file_vec
                            .iter()
                            .enumerate()
                            .map(|(i, fd)| {
                                let file = decls2.fd_file.get(i).unwrap();

                                Bool::and(ctx, &[param_fd._eq(fd), file._eq(&preopen.root.node)])
                            })
                            .collect_vec()
                            .as_slice(),
                    ),
                    | i @ _ => {
                        let prev_segment = segments.get(j).unwrap();
                        // TODO: segments_between
                        let segments_between = segments.get((j + 1)..i).unwrap();
                        let segments_between_are_separators = Bool::and(
                            ctx,
                            segments_between
                                .iter()
                                .map(|seg| separator.tester.apply(&[seg]).as_bool().unwrap())
                                .collect_vec()
                                .as_slice(),
                        );

                        Bool::and(
                            ctx,
                            &[
                                // Two components separated by separators.
                                &component.tester.apply(&[prev_segment]).as_bool().unwrap(),
                                &component.tester.apply(&[segment]).as_bool().unwrap(),
                                &segments_between_are_separators,
                                // prev segment maps to a file.
                                &segment_file_exists.apply(&[prev_segment]).as_bool().unwrap(),
                                // prev segment maps to the current directory.
                                &segment_file.apply(&[prev_segment])._eq(&preopen.root.node),
                            ],
                        )
                    },
                };

                clauses.push(Bool::or(
                    ctx,
                    &[
                        &Bool::and(
                            ctx,
                            &[
                                &frame,
                                &component.accessors[1]
                                    .apply(&[segment])
                                    .as_string()
                                    .unwrap()
                                    ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                                &segment_file_exists.apply(&[segment]).as_bool().unwrap(),
                                &segment_file.apply(&[segment])._eq(&preopen.root.node),
                            ],
                        ),
                        &Bool::and(
                            ctx,
                            &[
                                &frame,
                                &component.accessors[1]
                                    .apply(&[segment])
                                    .as_string()
                                    .unwrap()
                                    ._eq(&z3::ast::String::from_str(ctx, ".").unwrap())
                                    .not(),
                                &component.accessors[1]
                                    .apply(&[segment])
                                    .as_string()
                                    .unwrap()
                                    ._eq(&z3::ast::String::from_str(ctx, "..").unwrap())
                                    .not(),
                            ],
                        ),
                        &Bool::and(
                            ctx,
                            &[
                                &segment_file_exists.apply(&[segment]).as_bool().unwrap().not(),
                                &Bool::or(
                                    ctx,
                                    &[
                                        separator.tester.apply(&[segment]).as_bool().unwrap(),
                                        Bool::and(
                                            ctx,
                                            &[
                                                component.tester.apply(&[segment]).as_bool().unwrap(),
                                                component.accessors[1]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, ".").unwrap())
                                                    .not(),
                                                component.accessors[1]
                                                    .apply(&[segment])
                                                    .as_string()
                                                    .unwrap()
                                                    ._eq(&z3::ast::String::from_str(ctx, "..").unwrap())
                                                    .not(),
                                            ],
                                        ),
                                    ],
                                ),
                            ],
                        ),
                    ],
                ));
            }

            for (&_preopen_resource_idx, preopen) in decls.preopens.iter() {
                let mut dirs = vec![&preopen.root];

                while let Some(dir) = dirs.pop() {
                    for (filename, child) in dir.children.iter() {
                        let frame = match i {
                            | 0 => Bool::or(
                                ctx,
                                decls2
                                    .fd_file_vec
                                    .iter()
                                    .enumerate()
                                    .map(|(i, fd)| {
                                        let file = decls2.fd_file.get(i).unwrap();

                                        Bool::and(ctx, &[param_fd._eq(fd), file._eq(&dir.node)])
                                    })
                                    .collect_vec()
                                    .as_slice(),
                            ),
                            | i @ _ => {
                                let prev_segment = segments.get(j).unwrap();
                                // TODO: segments_between
                                let segments_between = segments.get((j + 1)..i).unwrap();
                                let segments_between_are_separators = Bool::and(
                                    ctx,
                                    segments_between
                                        .iter()
                                        .map(|seg| separator.tester.apply(&[seg]).as_bool().unwrap())
                                        .collect_vec()
                                        .as_slice(),
                                );

                                Bool::and(
                                    ctx,
                                    &[
                                        // Two components separated by separators.
                                        &component.tester.apply(&[prev_segment]).as_bool().unwrap(),
                                        &component.tester.apply(&[segment]).as_bool().unwrap(),
                                        &segments_between_are_separators,
                                        // prev segment maps to a file.
                                        &segment_file_exists.apply(&[prev_segment]).as_bool().unwrap(),
                                        // prev segment maps to the current directory.
                                        &segment_file.apply(&[prev_segment])._eq(&dir.node),
                                    ],
                                )
                            },
                        };
                        let segment_maps_to_parent = match parents.get(&dir.node) {
                            | Some(&parent) => Bool::and(
                                ctx,
                                &[
                                    &segment_file_exists.apply(&[segment]).as_bool().unwrap(),
                                    &segment_file.apply(&[segment])._eq(parent),
                                ],
                            ),
                            | None => Bool::from_bool(ctx, false),
                        };

                        clauses.push(Bool::or(
                            ctx,
                            &[
                                Bool::and(
                                    ctx,
                                    &[
                                        &frame,
                                        // filename matches segment.
                                        &component.accessors[1]
                                            .apply(&[segment])
                                            .as_string()
                                            .unwrap()
                                            ._eq(&z3::ast::String::from_str(ctx, filename).unwrap()),
                                        // Then the current segments maps to the child file.
                                        &segment_file_exists.apply(&[segment]).as_bool().unwrap(),
                                        &segment_file.apply(&[segment])._eq(child.node()),
                                    ],
                                ),
                                Bool::and(
                                    ctx,
                                    &[
                                        &frame,
                                        // filename is `.`
                                        &component.accessors[1]
                                            .apply(&[segment])
                                            .as_string()
                                            .unwrap()
                                            ._eq(&z3::ast::String::from_str(ctx, ".").unwrap()),
                                        // Then the current segments maps to the same dir.
                                        &segment_file_exists.apply(&[segment]).as_bool().unwrap(),
                                        &segment_file.apply(&[segment])._eq(&dir.node),
                                    ],
                                ),
                                Bool::and(
                                    ctx,
                                    &[
                                        &frame,
                                        // filename is `..`
                                        &component.accessors[1]
                                            .apply(&[segment])
                                            .as_string()
                                            .unwrap()
                                            ._eq(&z3::ast::String::from_str(ctx, "..").unwrap()),
                                        // Then the current segments maps to the dir's parent.
                                        &segment_maps_to_parent,
                                    ],
                                ),
                                Bool::and(
                                    ctx,
                                    &[
                                        &segment_file_exists.apply(&[segment]).as_bool().unwrap().not(),
                                        &Bool::or(
                                            ctx,
                                            &[
                                                separator.tester.apply(&[segment]).as_bool().unwrap(),
                                                Bool::and(
                                                    ctx,
                                                    &[
                                                        component.tester.apply(&[segment]).as_bool().unwrap(),
                                                        component.accessors[1]
                                                            .apply(&[segment])
                                                            .as_string()
                                                            .unwrap()
                                                            ._eq(&z3::ast::String::from_str(ctx, ".").unwrap())
                                                            .not(),
                                                        component.accessors[1]
                                                            .apply(&[segment])
                                                            .as_string()
                                                            .unwrap()
                                                            ._eq(&z3::ast::String::from_str(ctx, "..").unwrap())
                                                            .not(),
                                                    ],
                                                ),
                                            ],
                                        ),
                                    ],
                                ),
                            ],
                        ));

                        match child {
                            | FileEncoding::Directory(d) => dirs.push(d),
                            | _ => (),
                        }
                    }
                }
            }
        }
    }

    Dynamic::from_ast(&Bool::and(ctx, clauses.as_slice()))
}

#[derive(Debug)]
struct StateDecls<'ctx> {
    preopens:  BTreeMap<ResourceIdx, PreopenFsEncoding<'ctx>>,
    resources: BTreeMap<ResourceIdx, Dynamic<'ctx>>,
    params:    BTreeMap<String, ParamDecl<'ctx>>,
    to_solves: ToSolves<'ctx>,
}

#[derive(Debug)]
struct StateDecls2<'ctx> {
    fd_file:      Vec<Dynamic<'ctx>>,
    fd_file_vec:  Vec<Dynamic<'ctx>>,
    fd_file_map:  HashMap<Dynamic<'ctx>, usize>,
    children:     Vec<BTreeMap<String, Dynamic<'ctx>>>,
    children_vec: Vec<Dynamic<'ctx>>,
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

pub struct StatefulStrategy<'u, 'data, 'ctx> {
    ctx:      &'ctx z3::Context,
    u:        &'u mut Unstructured<'data>,
    preopens: BTreeMap<ResourceIdx, PathBuf>,
}

impl<'u, 'data, 'ctx> StatefulStrategy<'u, 'data, 'ctx> {
    pub fn new(
        u: &'u mut Unstructured<'data>,
        ctx: &'ctx z3::Context,
        preopens: BTreeMap<ResourceIdx, PathBuf>,
    ) -> Self {
        Self { ctx, u, preopens }
    }
}

impl<'u, 'data, 'ctx> CallStrategy for StatefulStrategy<'u, 'data, 'ctx> {
    fn select_function<'spec>(&mut self, spec: &'spec Spec, env: &Environment) -> Result<&'spec Function, eyre::Error> {
        let interface = spec.interfaces.get_by_key("wasi_snapshot_preview1").unwrap();
        let mut candidates = Vec::new();

        for (_name, function) in &interface.functions {
            let mut state = State::new();

            for (&idx, path) in &self.preopens {
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

            let types = StateTypes::new(self.ctx, spec);
            let decls = state.declare(
                ArbitraryOrPresolved::Arbitrary(self.u),
                spec,
                self.ctx,
                &types,
                env,
                function,
                None,
            );
            let decls2 = state.declare2(&decls);
            let solver = z3::Solver::new(self.ctx);

            solver.assert(&state.encode(
                self.ctx,
                env,
                &types,
                &decls,
                &decls2,
                spec,
                function,
                None,
                None,
                function.input_contract.as_ref(),
            ));

            match solver.check() {
                | z3::SatResult::Sat => candidates.push(function),
                | _ => continue,
            };
        }

        let function = *self.u.choose(&candidates).wrap_err("failed to choose a function")?;

        Ok(function)
    }

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &Environment,
    ) -> Result<Vec<HighLevelValue>, eyre::Error> {
        let mut state = State::new();

        for (&idx, path) in &self.preopens {
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

        let types = StateTypes::new(self.ctx, spec);
        let decls = state.declare(
            ArbitraryOrPresolved::Arbitrary(self.u),
            spec,
            self.ctx,
            &types,
            env,
            function,
            None,
        );
        let decls2 = state.declare2(&decls);
        let solver = z3::Solver::new(self.ctx);
        let mut solver_params = z3::Params::new(self.ctx);

        solver_params.set_u32("sat.random_seed", self.u.arbitrary()?);
        solver_params.set_u32("smt.random_seed", self.u.arbitrary()?);
        solver.set_params(&solver_params);

        let mut solutions = Vec::new();
        let mut nsolutions = 0;

        solver.push();
        solver.assert(&state.encode(
            self.ctx,
            &env,
            &types,
            &decls,
            &decls2,
            spec,
            function,
            None,
            None,
            function.input_contract.as_ref(),
        ));

        loop {
            if solver.check() != z3::SatResult::Sat || nsolutions == 16 {
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
                            self.ctx,
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
            solver.push();
            solver.assert(&Bool::or(self.ctx, clauses.as_slice()));
        }

        let model = self.u.choose(&solutions).unwrap();
        let mut params = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            let tdef = param.tref.resolve(spec);
            let param_node_value = decls.params.get(&param.name).unwrap();
            let (wasi_value, resource_idx) =
                state.decode_to_wasi_value(self.ctx, spec, &types, &tdef, &param_node_value, &model);

            match resource_idx {
                | Some(resource_idx) => {
                    params.push(HighLevelValue::Resource(resource_idx));
                },
                | None => params.push(HighLevelValue::Concrete(wasi_value)),
            }
        }

        Ok(params)
    }

    fn handle_results(
        &mut self,
        spec: &Spec,
        function: &Function,
        env: &mut Environment,
        params: Vec<HighLevelValue>,
        results: Vec<Option<ResourceIdx>>,
        result_values: Option<&[WasiValue]>,
    ) -> Result<(), eyre::Error> {
        let mut state = State::new();

        for (&idx, path) in &self.preopens {
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
                let s = String::from_utf8(env.resolve_value(param).string().unwrap().to_vec()).unwrap();
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
                    last_i = i + 1;
                    i += 1;
                }

                if last_i < i {
                    segments.push(Segment::Component(&s[last_i..i]));
                }

                (function_param.name.clone(), segments.len())
            })
            .collect();
        let types = StateTypes::new(self.ctx, spec);
        let decls = state.declare(
            ArbitraryOrPresolved::Presolved(lens),
            spec,
            self.ctx,
            &types,
            env,
            function,
            function.output_contract.as_ref(),
        );
        let decls2 = state.declare2(&decls);
        let solver = z3::Solver::new(self.ctx);

        solver.assert(&state.encode(
            self.ctx,
            &env,
            &types,
            &decls,
            &decls2,
            spec,
            function,
            Some(&params),
            result_values,
            function.output_contract.as_ref(),
        ));

        // Concretize the param values.
        for (i, function_param) in function.params.iter().enumerate() {
            let tdef = function_param.tref.resolve(spec);
            let param_node = decls.params.get(&function_param.name).unwrap();
            let hl_value = params.get(i).unwrap();

            match hl_value {
                | &HighLevelValue::Resource(resource_idx) => {
                    let value = &env.resources.get(resource_idx).unwrap().state;

                    solver.assert(&types.encode_wasi_value_decl(
                        self.ctx,
                        spec,
                        param_node,
                        &tdef,
                        value,
                        Some(resource_idx),
                    ));
                },
                | HighLevelValue::Concrete(value) => {
                    solver.assert(&types.encode_wasi_value_decl(self.ctx, spec, param_node, &tdef, value, None));
                },
            }
        }

        match solver.check() {
            | z3::SatResult::Sat => (),
            | _ => {
                return Err(err!(
                    "failed to solve output contract {:?}",
                    std::thread::current().name()
                ));
            },
        }

        let model = solver.get_model().unwrap();
        let mut clauses = Vec::new();

        for (name, param) in &decls.to_solves.params {
            let param_value = model.eval(param, true).unwrap().simplify();
            let (param_idx, function_param) = function
                .params
                .iter()
                .enumerate()
                .find(|(_, param)| &param.name == name)
                .unwrap();
            let tdef = function_param.tref.resolve(spec);
            let wasi_value = state.decode_to_wasi_value_inner(
                self.ctx,
                spec,
                &types,
                &tdef,
                &ParamDecl::Node(param_value.clone()),
                &model,
            );
            let param_resource_idx = match params.get(param_idx).unwrap() {
                | HighLevelValue::Resource(resource_idx) => *resource_idx,
                | HighLevelValue::Concrete(_wasi_value) => todo!(),
            };
            let resource = env.resources.get_mut(param_resource_idx).unwrap();

            resource.state = wasi_value;
            clauses.push(param._eq(&param_value).not());
        }

        for (name, result) in &decls.to_solves.results {
            let result_value = model.eval(result, true).unwrap().simplify();
            let (result_idx, function_result) = function
                .results
                .iter()
                .enumerate()
                .find(|(_, result)| &result.name == name)
                .unwrap();
            let tdef = function_result.tref.resolve(spec);
            let wasi_value = state.decode_to_wasi_value_inner(
                self.ctx,
                spec,
                &types,
                &tdef,
                &ParamDecl::Node(result_value.clone()),
                &model,
            );
            let result_resource_idx = results.get(result_idx).unwrap().unwrap();
            let resource = env.resources.get_mut(result_resource_idx).unwrap();

            resource.state = wasi_value;
            clauses.push(result._eq(&result_value).not());
        }

        solver.push();
        solver.assert(&Bool::or(self.ctx, clauses.as_slice()));

        match solver.check() {
            | z3::SatResult::Unsat => (),
            | _ => {
                return Err(err!("more than one solution for output contract"));
            },
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
            root: Directory::ingest_abs(path)?,
        })
    }
}

#[derive(Clone, Debug)]
struct PreopenFsEncoding<'ctx> {
    root: DirectoryEncoding<'ctx>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum File {
    Directory(Directory),
    RegularFile(RegularFile),
    Symlink(Symlink),
}

impl File {
    fn declare<'ctx>(&self, ctx: &'ctx z3::Context, types: &StateTypes<'ctx>) -> FileEncoding<'ctx> {
        match self {
            | File::Directory(directory) => {
                let dir = directory.declare(ctx, types);

                FileEncoding::Directory(dir)
            },
            | File::RegularFile(regular_file) => FileEncoding::RegularFile(regular_file.declare(ctx, types)),
            | File::Symlink(symlink) => FileEncoding::Symlink(symlink.declare(ctx, types)),
        }
    }

    fn ingest(dir: &mut fs::File, path: &Path) -> Result<Self, eyre::Error> {
        let file = fs_at::OpenOptions::default()
            .read(true)
            .follow(false)
            .open_at(dir, path)
            .wrap_err(format!("file `{}`", path.display()))?;
        let metadata = file.metadata()?;
        let file_type = metadata.file_type();

        if file_type.is_dir() {
            Ok(Self::Directory(Directory::ingest(dir, path)?))
        } else if file_type.is_file() {
            Ok(Self::RegularFile(RegularFile::ingest(dir, path)?))
        } else if file_type.is_symlink() {
            Ok(Self::Symlink(Symlink::ingest(dir, path)?))
        } else {
            unimplemented!("unsupported file type")
        }
    }
}

#[derive(Clone, Debug)]
enum FileEncoding<'ctx> {
    Directory(DirectoryEncoding<'ctx>),
    RegularFile(RegularFileEncoding<'ctx>),
    Symlink(SymlinkEncoding<'ctx>),
}

impl<'ctx> FileEncoding<'ctx> {
    fn node(&self) -> &Dynamic {
        match self {
            | FileEncoding::Directory(d) => &d.node,
            | FileEncoding::RegularFile(f) => &f.node,
            | FileEncoding::Symlink(l) => &l.node,
        }
    }

    fn as_ref(&self) -> FileEncodingRef<'ctx, '_> {
        match self {
            | FileEncoding::Directory(d) => FileEncodingRef::Directory(d),
            | FileEncoding::RegularFile(f) => FileEncodingRef::RegularFile(f),
            | FileEncoding::Symlink(l) => FileEncodingRef::Symlink(l),
        }
    }
}

#[derive(Debug)]
enum FileEncodingRef<'ctx, 'a> {
    Directory(&'a DirectoryEncoding<'ctx>),
    RegularFile(&'a RegularFileEncoding<'ctx>),
    Symlink(&'a SymlinkEncoding<'ctx>),
}

impl<'ctx, 'a> FileEncodingRef<'ctx, 'a> {
    fn directory(&self) -> Option<&'a DirectoryEncoding<'ctx>> {
        match self {
            | &FileEncodingRef::Directory(d) => Some(d),
            | _ => None,
        }
    }

    fn node(&self) -> &Dynamic<'ctx> {
        match self {
            | FileEncodingRef::Directory(d) => &d.node,
            | FileEncodingRef::RegularFile(f) => &f.node,
            | FileEncodingRef::Symlink(l) => &l.node,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Directory {
    children: IndexSpace<String, File>,
}

impl Directory {
    fn declare<'ctx>(&self, ctx: &'ctx z3::Context, types: &StateTypes<'ctx>) -> DirectoryEncoding<'ctx> {
        let node = Dynamic::fresh_const(ctx, "file--", &types.file.sort);
        let mut children = BTreeMap::new();

        for (name, child) in self.children.iter() {
            let child = child.declare(ctx, types);

            children.insert(name.to_owned(), child);
        }

        DirectoryEncoding { node, children }
    }

    fn ingest_abs(path: &Path) -> Result<Self, eyre::Error> {
        let mut paths: Vec<PathBuf> = Default::default();

        for entry in fs::read_dir(path).wrap_err("failed to read dir")? {
            let entry = entry?;

            paths.push(PathBuf::from(entry.file_name()));
        }

        paths.sort();

        let mut children = IndexSpace::new();
        let mut open_options = fs::OpenOptions::new();

        open_options.read(true);

        if cfg!(windows) {
            open_options.custom_flags(0x02000000);
        }

        let mut dir = open_options.open(path)?;

        for path in &paths {
            let file = File::ingest(&mut dir, &path)?;

            children.push(
                String::from_utf8(path.file_name().unwrap().as_encoded_bytes().to_vec()).unwrap(),
                file,
            );
        }

        Ok(Self { children })
    }

    fn ingest(dir: &mut fs::File, path: &Path) -> Result<Self, eyre::Error> {
        let mut open_options = fs_at::OpenOptions::default();

        open_options.read(true);

        let mut dir = open_options.open_dir_at(dir, path)?;
        let mut paths: Vec<PathBuf> = Default::default();

        for entry in fs_at::read_dir(&mut dir).wrap_err("failed to read dir")? {
            let entry = entry?;

            if entry.name() == "." || entry.name() == ".." {
                continue;
            }

            paths.push(PathBuf::from(entry.name()));
        }

        paths.sort();

        let mut children = IndexSpace::new();

        for path in &paths {
            let file = File::ingest(&mut dir, &path)?;

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
    params:  BTreeMap<String, Dynamic<'ctx>>,
    results: BTreeMap<String, Dynamic<'ctx>>,
}

#[derive(Clone, Debug)]
struct DirectoryEncoding<'ctx> {
    node:     Dynamic<'ctx>,
    children: BTreeMap<String, FileEncoding<'ctx>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct RegularFile {
    size: u64,
}

impl RegularFile {
    fn declare<'ctx>(&self, ctx: &'ctx z3::Context, types: &StateTypes<'ctx>) -> RegularFileEncoding<'ctx> {
        let node = Dynamic::fresh_const(ctx, "file--", &types.file.sort);

        RegularFileEncoding { node, size: self.size }
    }

    fn ingest(dir: &fs::File, path: &Path) -> Result<Self, io::Error> {
        let file = fs_at::OpenOptions::default().read(true).open_at(dir, path)?;

        Ok(Self {
            size: file.metadata()?.len() as u64,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Symlink(String);

impl Symlink {
    fn declare<'ctx>(&self, ctx: &'ctx z3::Context, types: &StateTypes<'ctx>) -> SymlinkEncoding<'ctx> {
        let node = Dynamic::fresh_const(ctx, "file--", &types.file.sort);

        SymlinkEncoding {
            node,
            content: self.0.clone(),
        }
    }

    fn ingest(dir: &fs::File, path: &Path) -> Result<Self, io::Error> {
        let mut file = fs_at::OpenOptions::default().follow(false).open_at(dir, path)?;
        let mut link = String::new();

        file.read_to_string(&mut link)?;

        Ok(Self(link))
    }
}

#[derive(Clone, Debug)]
struct RegularFileEncoding<'ctx> {
    node: Dynamic<'ctx>,
    size: u64,
}

#[derive(Clone, Debug)]
struct SymlinkEncoding<'ctx> {
    node:    Dynamic<'ctx>,
    content: String,
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

    fn unwrap_ast_as_int<'ctx>(&self, types: &'ctx StateTypes<'ctx>, ast: &Dynamic<'ctx>) -> Int<'ctx> {
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
