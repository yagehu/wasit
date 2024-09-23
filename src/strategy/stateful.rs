use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io,
    path::{Path, PathBuf},
};

use arbitrary::Unstructured;
use eyre::Context;
use idxspace::IndexSpace;
use itertools::Itertools;
use petgraph::{data::DataMap as _, graph::DiGraph, visit::IntoNeighborsDirected};
use z3::ast::Ast;

use super::CallStrategy;
use crate::{
    spec::{FlagsValue, Function, RecordValue, Spec, TypeDef, VariantValue, WasiType, WasiValue},
    Environment,
    ResourceIdx,
    RuntimeContext,
};

#[derive(Clone, Debug)]
struct State {
    preopens:  IndexSpace<ResourceIdx, PreopenFs>,
    paths:     BTreeMap<String, PathString>,
    fds_graph: DiGraph<ResourceIdx, String>,
    fds_idxs:  HashMap<ResourceIdx, petgraph::graph::NodeIndex>,
    resources: BTreeMap<ResourceIdx, WasiValue>,
}

impl State {
    fn new() -> Self {
        Self {
            preopens:  Default::default(),
            paths:     Default::default(),
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
        ctx: &'ctx z3::Context,
        types: &StateTypes<'ctx>,
        env: &Environment,
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
                z3::ast::Dynamic::fresh_const(
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
                &[
                    &types.file.sort,
                    &z3::Sort::string(ctx),
                    &types.segment.sort,
                ],
                &z3::Sort::bool(ctx),
            ),
            preopens,
            resources,
        }
    }

    fn encode<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        env: &Environment,
        spec: &Spec,
        types: &'ctx StateTypes<'ctx>,
        decls: &StateDecls<'ctx>,
    ) -> z3::ast::Bool<'ctx> {
        let mut clauses = Vec::new();
        let fds_graph_rev = petgraph::visit::Reversed(&self.fds_graph);
        let mut topo = petgraph::visit::Topo::new(&fds_graph_rev);
        let mut dirs: HashMap<ResourceIdx, &DirectoryEncoding> = Default::default();
        let mut fd_file_pairs = Vec::new();

        for (&resource_idx, preopen) in decls.preopens.iter() {
            dirs.insert(resource_idx, &preopen.root);
        }

        for (&idx, preopen) in decls.preopens.iter() {
            fd_file_pairs.push((decls.resources.get(&idx).unwrap(), &preopen.root.node));
        }

        while let Some(node_idx) = topo.next(fds_graph_rev) {
            let fd_resource_idx = *fds_graph_rev.node_weight(node_idx).unwrap();
            let fd = decls.resources.get(&fd_resource_idx).unwrap();
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

        // fd -> file mapping
        {
            let some_fd =
                z3::ast::Dynamic::fresh_const(ctx, "", &types.resources.get("fd").unwrap().sort);
            let some_file = z3::ast::Dynamic::fresh_const(ctx, "", &types.file.sort);

            clauses.push(z3::ast::forall_const(
                ctx,
                &[&some_fd, &some_file],
                &[],
                &z3::ast::Bool::or(
                    ctx,
                    fd_file_pairs
                        .into_iter()
                        .map(|(fd, file)| {
                            z3::ast::Bool::and(ctx, &[fd._eq(&some_fd), file._eq(&some_file)])
                        })
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
            clauses.push(z3::ast::forall_const(
                ctx,
                &[],
                &[],
                &z3::ast::Bool::and(
                    ctx,
                    self.resources
                        .iter()
                        .map(|(idx, resource_value)| {
                            let tdef = spec
                                .types
                                .get_by_key(env.resources_types.get(idx).unwrap())
                                .unwrap();
                            let resource_node = decls.resources.get(idx).unwrap();

                            types.encode_wasi_value(ctx, spec, resource_node, tdef, resource_value)
                        })
                        .collect_vec()
                        .as_slice(),
                ),
            ));
        }

        z3::ast::Bool::and(ctx, &clauses)
    }

    fn decode_to_wasi_value(
        &self,
        spec: &Spec,
        types: &StateTypes,
        tdef: &TypeDef,
        node: &z3::ast::Dynamic,
    ) -> WasiValue {
        let datatype = types.resources.get(&tdef.name).unwrap();
        let wasi_type = match &tdef.state {
            | Some(t) => t,
            | None => &tdef.wasi,
        };

        match wasi_type {
            | WasiType::S64 => WasiValue::S64(
                datatype.variants[0].accessors[0]
                    .apply(&[node])
                    .simplify()
                    .as_int()
                    .unwrap()
                    .as_i64()
                    .unwrap(),
            ),
            | WasiType::U8 => todo!(),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => todo!(),
            | WasiType::U64 => WasiValue::U64(
                datatype.variants[0].accessors[0]
                    .apply(&[node])
                    .simplify()
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
                        datatype.variants[0].accessors[i]
                            .apply(&[node])
                            .simplify()
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
                    if variant
                        .tester
                        .apply(&[node])
                        .simplify()
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
                        spec,
                        types,
                        payload_tref.resolve(spec),
                        &datatype.variants[case_idx].accessors[0].apply(&[node]),
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
                        println!("{:#?}", node);
                        self.decode_to_wasi_value(
                            spec,
                            types,
                            member.tref.resolve(spec),
                            &datatype.variants[0].accessors[i].apply(&[node]),
                        )
                    })
                    .collect_vec(),
            }),
            | WasiType::String => WasiValue::String(
                datatype.variants[0].accessors[0]
                    .apply(&[node])
                    .simplify()
                    .as_string()
                    .unwrap()
                    .as_string()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            ),
            | WasiType::List(list_type) => todo!(),
        }
    }
}

#[derive(Debug)]
struct StateTypes<'ctx> {
    resources: HashMap<String, z3::DatatypeSort<'ctx>>,
    file:      z3::DatatypeSort<'ctx>,
    segment:   z3::DatatypeSort<'ctx>,
}

impl<'ctx> StateTypes<'ctx> {
    fn new(ctx: &'ctx z3::Context, spec: &Spec) -> Self {
        let mut resources = HashMap::new();

        fn encode_type<'ctx>(
            ctx: &'ctx z3::Context,
            spec: &Spec,
            name: &str,
            tdef: &TypeDef,
            resource_types: &mut HashMap<String, z3::DatatypeSort<'ctx>>,
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
                | WasiType::String => datatype.variant(
                    name,
                    vec![(name, z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
                ),
                | WasiType::List(list_type) => {
                    let tdef = list_type.item.resolve(spec);

                    encode_type(ctx, spec, &tdef.name, tdef, resource_types);
                    datatype.variant(
                        name,
                        vec![(
                            name,
                            z3::DatatypeAccessor::Sort(z3::Sort::array(
                                ctx,
                                &z3::Sort::int(ctx),
                                &resource_types.get(&tdef.name).unwrap().sort,
                            )),
                        )],
                    )
                },
            };

            resource_types.insert(tdef.name.clone(), datatype.finish());
        }

        for (name, tdef) in spec.types.iter() {
            encode_type(ctx, spec, name, tdef, &mut resources);
        }

        Self {
            resources,
            file: z3::DatatypeBuilder::new(ctx, "file")
                .variant("directory", vec![])
                .variant("regular-file", vec![])
                .finish(),
            segment: z3::DatatypeBuilder::new(ctx, "segment")
                .variant("separator", vec![])
                .variant(
                    "component",
                    vec![("string", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
                )
                .finish(),
        }
    }

    fn encode_wasi_value(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec,
        node: &dyn z3::ast::Ast<'ctx>,
        tdef: &TypeDef,
        value: &WasiValue,
    ) -> z3::ast::Bool<'ctx> {
        let datatype = self.resources.get(&tdef.name).unwrap();
        let ty = match &tdef.state {
            | Some(t) => t,
            | None => &tdef.wasi,
        };

        match (ty, value) {
            | (_, &WasiValue::U8(i)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U16(i)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U32(i)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_u64(ctx, i.into())),
            | (_, &WasiValue::U64(i)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_u64(ctx, i)),
            | (_, &WasiValue::S64(i)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_i64(ctx, i)),
            | (_, &WasiValue::Handle(handle)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_int()
                .unwrap()
                ._eq(&z3::ast::Int::from_u64(ctx, handle.into())),
            | (WasiType::Record(record), WasiValue::Record(record_value)) => z3::ast::Bool::and(
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
                            &datatype.variants[0].accessors[i].apply(&[node]),
                            member.tref.resolve(spec),
                            member_value,
                        )
                    })
                    .collect_vec()
                    .as_slice(),
            ),
            | (_, WasiValue::Record(_)) => unreachable!(),
            | (WasiType::Flags(flags), WasiValue::Flags(flags_value)) => z3::ast::Bool::and(
                ctx,
                flags
                    .fields
                    .iter()
                    .enumerate()
                    .zip(flags_value.fields.iter())
                    .map(|((i, _name), &value)| {
                        datatype.variants[0].accessors[i]
                            .apply(&[node])
                            .as_bool()
                            .unwrap()
                            ._eq(&z3::ast::Bool::from_bool(ctx, value))
                    })
                    .collect_vec()
                    .as_slice(),
            ),
            | (_, WasiValue::Flags(_)) => unreachable!(),
            | (_, WasiValue::String(string)) => datatype.variants[0].accessors[0]
                .apply(&[node])
                .as_string()
                .unwrap()
                ._eq(
                    &z3::ast::String::from_str(
                        ctx,
                        String::from_utf8(string.to_vec()).unwrap().as_str(),
                    )
                    .unwrap(),
                ),
            | (WasiType::Variant(variant), WasiValue::Variant(variant_value)) => {
                match &variant_value.payload {
                    | Some(payload) => {
                        let payload_tdef = variant.cases[variant_value.case_idx]
                            .payload
                            .as_ref()
                            .unwrap()
                            .resolve(spec);

                        z3::ast::Bool::and(
                            ctx,
                            &[
                                datatype.variants[variant_value.case_idx]
                                    .tester
                                    .apply(&[node])
                                    .as_bool()
                                    .unwrap(),
                                self.encode_wasi_value(
                                    ctx,
                                    spec,
                                    &datatype.variants[variant_value.case_idx].accessors[0]
                                        .apply(&[node]),
                                    payload_tdef,
                                    payload,
                                ),
                            ],
                        )
                    },
                    | None => datatype.variants[variant_value.case_idx]
                        .tester
                        .apply(&[node])
                        .as_bool()
                        .unwrap(),
                }
            },
            | (_, WasiValue::Variant(variant_value)) => unreachable!(),
            | (_, WasiValue::List(list_value)) => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct StateDecls<'ctx> {
    fd_file:   z3::FuncDecl<'ctx>,
    children:  z3::FuncDecl<'ctx>,
    preopens:  BTreeMap<ResourceIdx, PreopenFsEncoding<'ctx>>,
    resources: BTreeMap<ResourceIdx, z3::ast::Dynamic<'ctx>>,
}

impl<'ctx> StateDecls<'ctx> {
}

// #[derive(Debug)]
// struct NoNonexistentDirBacktrack<'ctx> {
//     clauses:               Vec<z3::ast::Bool<'ctx>>,
//     segment_file_relation: z3::FuncDecl<'ctx>,
// }

// impl<'ctx> NoNonexistentDirBacktrack<'ctx> {
//     fn new(
//         ctx: &'ctx z3::Context,
//         state_decl: &mut StateDecl<'ctx, '_>,
//         fd_idx: ResourceIdx,
//         path_param_name: &str,
//     ) -> Self {
//         let namespace = format!("nndb-{}-{}", fd_idx.0, path_param_name);
//         let option_file = z3::DatatypeBuilder::new(ctx, format!("{namespace}--option-file"))
//             .variant("none", vec![])
//             .variant(
//                 "some",
//                 vec![(
//                     "some",
//                     z3::DatatypeAccessor::Sort(state_decl.state.z3_file_type.sort.clone()),
//                 )],
//             )
//             .finish();
//         let preopen = state_decl.preopens.get_by_key(&fd_idx).unwrap();
//         let path_string = state_decl.paths.get(path_param_name).unwrap();
//         let segment_file_relation = z3::FuncDecl::new(
//             ctx,
//             format!("{}--segment-file", namespace),
//             &[
//                 &state_decl.state.z3_segment_type.sort,
//                 &state_decl.state.z3_file_type.sort,
//             ],
//             &z3::Sort::bool(ctx),
//         );
//         let mut clauses: Vec<_> = Default::default();

//         clauses.push(
//             segment_file_relation
//                 .apply(&[
//                     &path_string.segments.first().unwrap().node,
//                     &preopen.root.node,
//                 ])
//                 .as_bool()
//                 .unwrap(),
//         );

//         let mut prev_option_file = z3::ast::Dynamic::fresh_const(ctx, "", &option_file.sort);

//         // We always start with a valid fd that maps to a file.
//         clauses.push(
//             // option_file.variants[1]
//             //     .tester
//             //     .apply(&[&prev_option_file])
//             //     .as_bool()
//             //     .unwrap(),
//             todo!("we also need to constrain the option file to the actual file mapped by the fd"),
//         );

//         for (i, segment) in path_string.segments.iter().enumerate() {
//             let next_option_file = z3::ast::Dynamic::fresh_const(ctx, "", &option_file.sort);
//             let some_file =
//                 z3::ast::Dynamic::fresh_const(ctx, "", &state_decl.state.z3_file_type.sort);

//             clauses.push(z3::ast::exists_const(
//                 ctx,
//                 &[&some_file],
//                 &[],
//                 &z3::ast::Bool::and(
//                     ctx,
//                     &[
//                         option_file.variants[1]
//                             .tester
//                             .apply(&[&next_option_file])
//                             .as_bool()
//                             .unwrap(),
//                         option_file.variants[1].accessors[0]
//                             .apply(&[&next_option_file])
//                             ._eq(&some_file),
//                     ],
//                 )
//                 .iff(&z3::ast::Bool::and(
//                     ctx,
//                     &[
//                         &state_decl.state.z3_segment_type.variants[1]
//                             .tester
//                             .apply(&[&segment.node])
//                             .as_bool()
//                             .unwrap(),
//                         todo!(),
//                         // state_decl.state.z3_segment_type.variants[1].accessors[0]
//                         //     .apply(&[&segment.node])
//                         //     .as_string()
//                         //     .unwrap(),
//                     ],
//                 )),
//             ));

//             prev_option_file = next_option_file;
//         }

//         Self {
//             clauses,
//             segment_file_relation,
//         }
//     }
// }

pub struct StatefulStrategy<'u, 'data, 'env, 'ctx, 'zctx> {
    z3_ctx: &'zctx z3::Context,
    u:      &'u mut Unstructured<'data>,
    env:    &'env Environment,
    ctx:    &'ctx RuntimeContext,
}

impl<'u, 'data, 'env, 'ctx, 'zctx> StatefulStrategy<'u, 'data, 'env, 'ctx, 'zctx> {
    pub fn new(
        u: &'u mut Unstructured<'data>,
        env: &'env Environment,
        ctx: &'ctx RuntimeContext,
        z3_ctx: &'zctx z3::Context,
    ) -> Self {
        Self {
            z3_ctx,
            u,
            env,
            ctx,
        }
    }
}

impl CallStrategy for StatefulStrategy<'_, '_, '_, '_, '_> {
    fn select_function<'spec>(
        &mut self,
        spec: &'spec Spec,
    ) -> Result<&'spec Function, eyre::Error> {
        todo!()
    }

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<(WasiValue, Option<ResourceIdx>)>, eyre::Error> {
        todo!()
    }
}

// #[derive(Debug)]
// struct State<'ctx> {
//     z3_file_type:    z3::DatatypeSort<'ctx>,
//     z3_segment_type: z3::DatatypeSort<'ctx>,

//     resource_types: IndexSpace<String, z3::DatatypeSort<'ctx>>,
//     preopens:       IndexSpace<ResourceIdx, PreopenFs>,
//     paths:          BTreeMap<String, PathString>,
//     fds:            BTreeMap<ResourceIdx, (ResourceIdx, PathBuf)>,
// }

// impl<'ctx> State<'ctx> {
//     pub fn new(ctx: &'ctx z3::Context, spec: &Spec) -> Self {
//         fn encode_type<'ctx>(
//             ctx: &'ctx z3::Context,
//             spec: &Spec,
//             name: &str,
//             tdef: &TypeDef,
//             resource_types: &mut IndexSpace<String, z3::DatatypeSort<'ctx>>,
//         ) {
//             if resource_types.get_by_key(name).is_some() {
//                 return;
//             }

//             let wasi_type = match &tdef.state {
//                 | Some(state) => state,
//                 | None => &tdef.wasi,
//             };
//             let mut datatype = z3::DatatypeBuilder::new(ctx, name);

//             datatype = match wasi_type {
//                 | WasiType::S64
//                 | WasiType::U8
//                 | WasiType::U16
//                 | WasiType::U32
//                 | WasiType::U64
//                 | WasiType::Handle => datatype.variant(
//                     name,
//                     vec![(name, z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
//                 ),
//                 | WasiType::Flags(flags_type) => datatype.variant(
//                     name,
//                     flags_type
//                         .fields
//                         .iter()
//                         .map(|field| {
//                             (
//                                 field.as_str(),
//                                 z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)),
//                             )
//                         })
//                         .collect_vec(),
//                 ),
//                 | WasiType::Variant(variant_type) => {
//                     for case in &variant_type.cases {
//                         let fields = match &case.payload {
//                             | Some(payload) => {
//                                 let payload_tdef = payload.resolve(spec);

//                                 encode_type(
//                                     ctx,
//                                     spec,
//                                     &payload_tdef.name,
//                                     payload_tdef,
//                                     resource_types,
//                                 );

//                                 vec![(
//                                     "payload",
//                                     z3::DatatypeAccessor::Sort(
//                                         resource_types
//                                             .get_by_key(&payload_tdef.name)
//                                             .unwrap()
//                                             .sort
//                                             .clone(),
//                                     ),
//                                 )]
//                             },
//                             | None => vec![],
//                         };

//                         datatype = datatype.variant(&case.name, fields);
//                     }

//                     datatype
//                 },
//                 | WasiType::Record(record_type) => datatype.variant(
//                     name,
//                     record_type
//                         .members
//                         .iter()
//                         .map(|member| {
//                             let member_tdef = member.tref.resolve(spec);

//                             encode_type(
//                                 ctx,
//                                 spec,
//                                 &member_tdef.name,
//                                 member.tref.resolve(spec),
//                                 resource_types,
//                             );

//                             let member_datatype = resource_types
//                                 .get_by_key(&member.tref.resolve(spec).name)
//                                 .unwrap();
//                             (
//                                 member.name.as_str(),
//                                 z3::DatatypeAccessor::Sort(member_datatype.sort.clone()),
//                             )
//                         })
//                         .collect_vec(),
//                 ),
//                 | WasiType::String => datatype.variant(
//                     name,
//                     vec![(name, z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
//                 ),
//                 | WasiType::List(list_type) => {
//                     let tdef = list_type.item.resolve(spec);

//                     encode_type(ctx, spec, &tdef.name, tdef, resource_types);
//                     datatype.variant(
//                         name,
//                         vec![(
//                             name,
//                             z3::DatatypeAccessor::Sort(z3::Sort::array(
//                                 ctx,
//                                 &z3::Sort::int(ctx),
//                                 &resource_types.get_by_key(&tdef.name).unwrap().sort,
//                             )),
//                         )],
//                     )
//                 },
//             };

//             resource_types.push(tdef.name.clone(), datatype.finish());
//         }

//         let mut resource_types: IndexSpace<String, z3::DatatypeSort> = Default::default();

//         for (name, tdef) in spec.types.iter() {
//             encode_type(ctx, spec, name, tdef, &mut resource_types);
//         }

//         Self {
//             z3_file_type: z3::DatatypeBuilder::new(ctx, "file")
//                 .variant("directory", vec![])
//                 .variant("regular-file", vec![])
//                 .finish(),
//             z3_segment_type: z3::DatatypeBuilder::new(ctx, "segment")
//                 .variant("separator", vec![])
//                 .variant(
//                     "component",
//                     vec![("string", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
//                 )
//                 .finish(),
//             resource_types,
//             preopens: Default::default(),
//             paths: Default::default(),
//             fds: Default::default(),
//         }
//     }

//     pub fn push_dir(&mut self, resource_idx: ResourceIdx, path: &Path) -> Result<(), eyre::Error> {
//         let dir = PreopenFs::new(path)?;

//         self.preopens.push(resource_idx, dir);

//         Ok(())
//     }

//     pub fn push_path(&mut self, param_name: String, path_string: PathString) {
//         self.paths.insert(param_name, path_string);
//     }

//     pub fn declare(&'ctx self, ctx: &'ctx z3::Context) -> StateDecl<'ctx, '_> {
//         let mut preopens: IndexSpace<ResourceIdx, EncodedPreopenFs> = Default::default();
//         let mut children: Vec<_> = Default::default();
//         let mut files: Vec<_> = Default::default();
//         let mut paths: BTreeMap<_, _> = Default::default();
//         let children_relation = ChildrenRelation(z3::FuncDecl::new(
//             ctx,
//             "children",
//             &[
//                 &self.z3_file_type.sort,
//                 &z3::Sort::string(ctx),
//                 &self.z3_file_type.sort,
//             ],
//             &z3::Sort::bool(ctx),
//         ));

//         for (&resource_idx, preopen) in self.preopens.iter() {
//             let mut scope = PreopenFsEncodingScope::new(ctx, self, resource_idx);
//             let dir = preopen.root.encode(ctx, self, &mut scope);

//             preopens.push(resource_idx, EncodedPreopenFs { root: dir });
//             children.extend(scope.children);
//             files.extend(scope.files);
//         }

//         for (param_name, path_string) in &self.paths {
//             paths.insert(param_name.to_owned(), path_string.declare(ctx, self));
//         }

//         let fd = self.resource_types.get_by_key("fd").unwrap();

//         StateDecl {
//             state: self,
//             preopens,
//             files,
//             children,
//             children_relation,
//             paths,
//             fd_file: z3::FuncDecl::new(
//                 ctx,
//                 "fd-file",
//                 &[&fd.sort, &self.z3_file_type.sort],
//                 &z3::Sort::bool(ctx),
//             ),
//         }
//     }
// }

// #[derive(Debug)]
// struct StateDecl<'ctx, 'state> {
//     state:             &'state State<'ctx>,
//     preopens:          IndexSpace<ResourceIdx, EncodedPreopenFs<'ctx>>,
//     files:             Vec<EncodedFile<'ctx>>,
//     children:          Vec<(EncodedDirectory<'ctx>, String, EncodedFile<'ctx>)>,
//     children_relation: ChildrenRelation<'ctx>,
//     paths:             BTreeMap<String, EncodedPathString<'ctx>>,
//     fd_file:           z3::FuncDecl<'ctx>,
// }

// impl<'ctx> StateDecl<'ctx, '_> {
//     fn encode(&'ctx self, ctx: &'ctx z3::Context) -> StateEncoding<'ctx> {
//         let mut clauses: Vec<z3::ast::Bool> = Default::default();
//         let any_dir = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_file_type.sort);
//         let any_file = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_file_type.sort);
//         let some_name = z3::ast::String::fresh_const(ctx, "");

//         // Children relation.
//         clauses.push(z3::ast::forall_const(
//             ctx,
//             &[&any_dir, &any_file, &some_name],
//             &[],
//             &z3::ast::Bool::or(
//                 ctx,
//                 self.children
//                     .iter()
//                     .map(|(dir, name, file)| {
//                         z3::ast::Bool::and(
//                             ctx,
//                             &[
//                                 &dir.node._eq(&any_dir),
//                                 &file.node()._eq(&any_file),
//                                 &some_name._eq(&z3::ast::String::from_str(ctx, name).unwrap()),
//                             ],
//                         )
//                     })
//                     .collect_vec()
//                     .as_slice(),
//             )
//             .ite(
//                 &self
//                     .children_relation
//                     .has_child(ctx, &any_dir, &some_name, &any_file),
//                 &self
//                     .children_relation
//                     .has_child(ctx, &any_dir, &some_name, &any_file)
//                     .not(),
//             ),
//         ));

//         // File type.
//         self.files
//             .iter()
//             .map(|file| match file {
//                 | EncodedFile::Directory(d) => {
//                     clauses.push(
//                         self.state.z3_file_type.variants[0]
//                             .tester
//                             .apply(&[d.node()])
//                             .as_bool()
//                             .unwrap(),
//                     );
//                 },
//                 | EncodedFile::RegularFile(f) => {
//                     clauses.push(
//                         self.state.z3_file_type.variants[1]
//                             .tester
//                             .apply(&[f.node()])
//                             .as_bool()
//                             .unwrap(),
//                     );
//                 },
//             })
//             .collect_vec();

//         // Path segment components cannot be the empty string.
//         clauses.push(z3::ast::Bool::and(
//             ctx,
//             &self
//                 .paths
//                 .iter()
//                 .map(|(_param_name, path_string)| {
//                     path_string
//                         .segments
//                         .iter()
//                         .map(|segment| {
//                             z3::ast::Bool::and(
//                                 ctx,
//                                 &[self.state.z3_segment_type.variants[1]
//                                     .tester
//                                     .apply(&[&segment.node])
//                                     .as_bool()
//                                     .unwrap()],
//                             )
//                             .implies(&unsafe {
//                                 z3::ast::Int::wrap(
//                                     ctx,
//                                     z3_sys::Z3_mk_seq_length(
//                                         ctx.get_z3_context(),
//                                         self.state.z3_segment_type.variants[1].accessors[0]
//                                             .apply(&[&segment.node])
//                                             .as_string()
//                                             .unwrap()
//                                             .get_z3_ast(),
//                                     ),
//                                 )
//                                 ._eq(&z3::ast::Int::from_u64(ctx, 0))
//                                 .not()
//                             })
//                         })
//                         .collect_vec()
//                 })
//                 .flatten()
//                 .collect_vec(),
//         ));

//         // The first segment must be a component.
//         clauses.push(z3::ast::Bool::and(
//             ctx,
//             &self
//                 .paths
//                 .iter()
//                 .map(|(_param_name, path)| {
//                     self.state.z3_segment_type.variants[1]
//                         .tester
//                         .apply(&[&path.segments[0].node])
//                         .as_bool()
//                         .unwrap()
//                 })
//                 .collect_vec(),
//         ));

//         // Adjacent segments can't both be components.
//         clauses.push(z3::ast::Bool::and(
//             ctx,
//             &self
//                 .paths
//                 .iter()
//                 .map(|(_param_name, path)| {
//                     let mut subclauses = Vec::new();

//                     for (i, segment) in path.segments.iter().enumerate() {
//                         if i > 0 {
//                             subclauses.push(
//                                 self.state.z3_segment_type.variants[1]
//                                     .tester
//                                     .apply(&[&segment.node])
//                                     .as_bool()
//                                     .unwrap()
//                                     .implies(
//                                         &self.state.z3_segment_type.variants[1]
//                                             .tester
//                                             .apply(&[&path.segments.get(i - 1).unwrap().node])
//                                             .as_bool()
//                                             .unwrap()
//                                             .not(),
//                                     ),
//                             );
//                         }
//                     }

//                     subclauses
//                 })
//                 .flatten()
//                 .collect_vec(),
//         ));

//         // Components cannot contain slash "/".
//         clauses.push(z3::ast::Bool::and(
//             ctx,
//             self.paths
//                 .iter()
//                 .flat_map(|(_param_name, path)| &path.segments)
//                 .map(|segment| {
//                     self.state.z3_segment_type.variants[1]
//                         .tester
//                         .apply(&[&segment.node])
//                         .as_bool()
//                         .unwrap()
//                         .implies(
//                             &self.state.z3_segment_type.variants[1].accessors[0]
//                                 .apply(&[&segment.node])
//                                 .as_string()
//                                 .unwrap()
//                                 .contains(&z3::ast::String::from_str(ctx, "/").unwrap())
//                                 .not(),
//                         )
//                 })
//                 .collect_vec()
//                 .as_slice(),
//         ));

//         let fd = self.state.resource_types.get_by_key("fd").unwrap();
//         let some_fd = z3::ast::Dynamic::fresh_const(ctx, "", &fd.sort);
//         let some_file = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_file_type.sort);

//         for (&resource_idx, &(parent_idx, ref path)) in &self.state.fds {
//             let mut curr = parent_idx;
//             let mut paths = Vec::new();

//             loop {
//                 let &(next_idx, ref path) = self.state.fds.get(&curr).unwrap();

//                 if next_idx == curr {
//                     break;
//                 }

//                 paths.push(path);
//             }

//             // curr is now a preopen

//             let preopen = self.state.preopens.get_by_key(&curr).unwrap();
//             let mut curr_file = File::Directory(preopen.root.clone());

//             while let Some(path) = paths.pop() {
//                 let mut curr = &curr_file;

//                 for component in path.components() {
//                     let component = component.as_os_str().to_os_string();

//                     match curr {
//                         | File::Directory(directory) => {
//                             let (name, file) = directory
//                                 .children
//                                 .iter()
//                                 .find(|(name, file)| name == &component)
//                                 .unwrap();

//                             curr = file;
//                         },
//                         | File::RegularFile(regular_file) => panic!("dir expected"),
//                     }
//                 }

//                 curr_file = curr.to_owned();
//             }
//         }

//         clauses.push(z3::ast::forall_const(
//             ctx,
//             &[&some_fd, &some_file],
//             &[],
//             &z3::ast::Bool::or(
//                 ctx,
//                 self.state
//                     .fds
//                     .iter()
//                     .map(|(resource_idx, (parent_resource_idx, path))| todo!())
//                     .collect_vec()
//                     .as_slice(),
//             )
//             .ite(
//                 &self
//                     .fd_file
//                     .apply(&[&some_fd, &some_file])
//                     .as_bool()
//                     .unwrap(),
//                 &self
//                     .fd_file
//                     .apply(&[&some_fd, &some_file])
//                     .as_bool()
//                     .unwrap()
//                     .not(),
//             ),
//         ));

//         StateEncoding { clauses }
//     }
// }

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
        let node = z3::ast::Dynamic::fresh_const(ctx, "file--", &types.file.sort);
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

#[derive(Debug)]
struct DirectoryEncoding<'ctx> {
    node:     z3::ast::Dynamic<'ctx>,
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
        let node = z3::ast::Dynamic::fresh_const(ctx, "file--", &types.file.sort);

        RegularFileEncoding { node }
    }

    fn ingest(_path: &Path) -> Result<Self, io::Error> {
        Ok(Self {})
    }
}

#[derive(Debug)]
struct RegularFileEncoding<'ctx> {
    node: z3::ast::Dynamic<'ctx>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PathString {
    param_name: String,
    nsegments:  usize,
}

impl PathString {
    // fn declare<'ctx>(
    //     &self,
    //     ctx: &'ctx z3::Context,
    //     state: &State<'ctx>,
    // ) -> EncodedPathString<'ctx> {
    //     let mut segments = Vec::with_capacity(self.nsegments);

    //     for _i in 0..self.nsegments {
    //         segments.push(z3::ast::Dynamic::fresh_const(
    //             ctx,
    //             &format!("segment--{}--", self.param_name),
    //             &state.z3_segment_type.sort,
    //         ));
    //     }

    //     EncodedPathString { segments }
    // }
}

#[derive(Debug)]
struct EncodedPathString<'ctx> {
    segments: Vec<z3::ast::Dynamic<'ctx>>,
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path};

    use tempfile::tempdir;

    use super::*;
    use crate::{
        resource::{self, Resource, Resources},
        spec::RecordValue,
    };

    #[test]
    fn ok() {
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let solver = z3::Solver::new(&ctx);
        let spec = Spec::preview1(&ctx).unwrap();
        let tempdir = tempdir().unwrap();
        let mut env = Environment::new();

        fs::write(tempdir.path().join("file"), &[]).unwrap();
        fs::create_dir(tempdir.path().join("dir")).unwrap();
        fs::write(tempdir.path().join("dir").join("nested"), &[]).unwrap();

        let dir_resource_idx = env.new_resource(
            "fd".to_string(),
            Resource {
                state: WasiValue::Record(RecordValue {
                    members: vec![
                        WasiValue::U64(0),
                        spec.get_type("fdflags")
                            .unwrap()
                            .flags()
                            .unwrap()
                            .value(HashSet::new()),
                        spec.get_type("filetype")
                            .unwrap()
                            .variant()
                            .unwrap()
                            .value_from_name("directory", None)
                            .unwrap(),
                        WasiValue::String("".as_bytes().to_vec()),
                        WasiValue::U64(0),
                    ],
                }),
            },
        );
        let file_resource_idx = env.new_resource(
            "fd".to_string(),
            Resource {
                state: WasiValue::Record(RecordValue {
                    members: vec![
                        WasiValue::U64(0),
                        spec.get_type("fdflags")
                            .unwrap()
                            .flags()
                            .unwrap()
                            .value(HashSet::new()),
                        spec.get_type("filetype")
                            .unwrap()
                            .variant()
                            .unwrap()
                            .value_from_name("regular_file", None)
                            .unwrap(),
                        WasiValue::String("file".as_bytes().to_vec()),
                        WasiValue::U64(0),
                    ],
                }),
            },
        );
        let mut state = State::new();
        let types = StateTypes::new(&ctx, &spec);

        state.push_preopen(dir_resource_idx, tempdir.path());
        state.push_resource(
            dir_resource_idx,
            spec.types.get_by_key("fd").unwrap(),
            env.resources.get(dir_resource_idx).unwrap().state.clone(),
        );
        state.push_resource(
            file_resource_idx,
            spec.types.get_by_key("fd").unwrap(),
            env.resources.get(file_resource_idx).unwrap().state.clone(),
        );

        let decls = state.declare(&ctx, &types, &env);
        let clause = state.encode(&ctx, &env, &spec, &types, &decls);

        solver.assert(&clause);

        // state.push_path(
        //     "path".to_string(),
        //     PathString {
        //         param_name: "path".to_owned(),
        //         nsegments:  3,
        //     },
        // );

        assert_eq!(solver.check(), z3::SatResult::Sat);

        {
            solver.push();

            let fd_datatype = types.resources.get("fd").unwrap();
            let path_datatype = types.resources.get("path").unwrap();
            let some_fd = z3::ast::Dynamic::fresh_const(&ctx, "sol-fd", &fd_datatype.sort);

            solver.assert(&z3::ast::Bool::and(
                &ctx,
                &[
                    &fd_datatype.variants[0]
                        .tester
                        .apply(&[&some_fd])
                        .as_bool()
                        .unwrap(),
                    &path_datatype.variants[0].accessors[0]
                        .apply(&[&fd_datatype.variants[0].accessors[3]
                            .apply(&[&some_fd])
                            .as_datatype()
                            .unwrap()])
                        .as_string()
                        .unwrap()
                        ._eq(&z3::ast::String::from_str(&ctx, "file").unwrap()),
                    &z3::ast::Bool::or(
                        &ctx,
                        decls
                            .resources
                            .values()
                            .map(|node| some_fd._eq(node))
                            .collect_vec()
                            .as_slice(),
                    ),
                ],
            ));

            assert_eq!(solver.check(), z3::SatResult::Sat);

            let model = solver.get_model().unwrap();
            let fd_tdef = spec.types.get_by_key("fd").unwrap();
            let resource_value = state.decode_to_wasi_value(
                &spec,
                &types,
                fd_tdef,
                &model.eval(&some_fd, true).unwrap().simplify(),
            );
            let resource_idx = *env
                .reverse_resource_index
                .get("fd")
                .unwrap()
                .get(&resource_value)
                .unwrap();

            assert_eq!(resource_idx.0, 1);

            solver.pop(1);
        }

        // assert!(model
        //     .eval(
        //         &z3::ast::exists_const(
        //             &ctx,
        //             &[&some_file],
        //             &[],
        //             &state_decl.children_relation.has_child(
        //                 &ctx,
        //                 encoded_preopen_fs.root.node(),
        //                 &z3::ast::String::from_str(&ctx, "file").unwrap(),
        //                 &some_file,
        //             ),
        //         ),
        //         true,
        //     )
        //     .unwrap()
        //     .simplify()
        //     .as_bool()
        //     .unwrap());
        // assert!(!model
        //     .eval(
        //         &z3::ast::exists_const(
        //             &ctx,
        //             &[&some_file],
        //             &[],
        //             &state_decl.children_relation.has_child(
        //                 &ctx,
        //                 encoded_preopen_fs.root.node(),
        //                 &z3::ast::String::from_str(&ctx, "nonexistant").unwrap(),
        //                 &some_file,
        //             ),
        //         ),
        //         true,
        //     )
        //     .unwrap()
        //     .simplify()
        //     .as_bool()
        //     .unwrap());
        // let path = state_decl.paths.get("path").unwrap();

        // // The second path segment cannot be a component because the first segment
        // // is always a component.
        // solver.push();
        // solver.assert(
        //     &state.z3_segment_type.variants[1]
        //         .tester
        //         .apply(&[&path.segments[1].node])
        //         .as_bool()
        //         .unwrap(),
        // );
        // assert_eq!(solver.check(), z3::SatResult::Unsat);
        // solver.pop(1);

        // // Components cannot contain "/".
        // solver.push();
        // solver.assert(&z3::ast::Bool::and(
        //     &ctx,
        //     state_decl
        //         .paths
        //         .iter()
        //         .flat_map(|(_param_name, path)| &path.segments)
        //         .map(|segment| {
        //             state.z3_segment_type.variants[1]
        //                 .tester
        //                 .apply(&[&segment.node])
        //                 .as_bool()
        //                 .unwrap()
        //                 .implies(
        //                     &state.z3_segment_type.variants[1].accessors[0]
        //                         .apply(&[&segment.node])
        //                         .as_string()
        //                         .unwrap()
        //                         .contains(&z3::ast::String::from_str(&ctx, "/").unwrap()),
        //                 )
        //         })
        //         .collect_vec()
        //         .as_slice(),
        // ));
        // assert_eq!(solver.check(), z3::SatResult::Unsat);
        // solver.pop(1);

        // panic!()
    }
}
