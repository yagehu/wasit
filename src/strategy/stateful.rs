use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io,
    path::{Path, PathBuf},
};

use arbitrary::Unstructured;
use eyre::Context;
use idxspace::IndexSpace;
use itertools::Itertools;
use z3::ast::Ast;

use super::CallStrategy;
use crate::{
    spec::{Function, Spec, WasiValue},
    Environment,
    ResourceIdx,
    RuntimeContext,
};

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
    ) -> Result<Vec<WasiValue>, eyre::Error> {
        todo!()
    }
}

#[derive(Debug)]
struct State<'ctx> {
    z3_file_type:    z3::DatatypeSort<'ctx>,
    z3_segment_type: z3::DatatypeSort<'ctx>,

    preopens: IndexSpace<ResourceIdx, PreopenFs>,
    paths:    BTreeMap<String, PathString>,
}

impl<'ctx> State<'ctx> {
    pub fn new(ctx: &'ctx z3::Context) -> Self {
        Self {
            z3_file_type:    z3::DatatypeBuilder::new(ctx, "file")
                .variant("directory", vec![])
                .variant("regular-file", vec![])
                .finish(),
            z3_segment_type: z3::DatatypeBuilder::new(ctx, "segment")
                .variant("separator", vec![])
                .variant(
                    "component",
                    vec![("string", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
                )
                .finish(),
            preopens:        Default::default(),
            paths:           Default::default(),
        }
    }

    pub fn push_dir(&mut self, resource_idx: ResourceIdx, path: &Path) -> Result<(), eyre::Error> {
        let dir = PreopenFs::new(path)?;

        self.preopens.push(resource_idx, dir);

        Ok(())
    }

    pub fn push_path(&mut self, param_name: String, path_string: PathString) {
        self.paths.insert(param_name, path_string);
    }

    pub fn declare(&'ctx self, ctx: &'ctx z3::Context) -> DeclaredState<'ctx, '_> {
        let mut preopens: IndexSpace<ResourceIdx, EncodedPreopenFs> = Default::default();
        let mut children: Vec<_> = Default::default();
        let mut files: Vec<_> = Default::default();
        let mut paths: BTreeMap<_, _> = Default::default();

        for (&resource_idx, preopen) in self.preopens.iter() {
            let mut scope = PreopenFsEncodingScope::new(ctx, self, resource_idx);
            let dir = preopen.root.encode(ctx, self, &mut scope);

            preopens.push(resource_idx, EncodedPreopenFs { root: dir });
            children.extend(scope.children);
            files.extend(scope.files);
        }

        for (param_name, path_string) in &self.paths {
            paths.insert(param_name.to_owned(), path_string.declare(ctx, self));
        }

        DeclaredState {
            state: self,
            preopens,
            files,
            children,
            paths,
        }
    }
}

#[derive(Debug)]
struct DeclaredState<'ctx, 'state> {
    state:    &'state State<'ctx>,
    preopens: IndexSpace<ResourceIdx, EncodedPreopenFs<'ctx>>,
    files:    Vec<EncodedFile<'ctx>>,
    children: Vec<(EncodedDirectory<'ctx>, String, EncodedFile<'ctx>)>,
    paths:    BTreeMap<String, EncodedPathString<'ctx>>,
}

impl<'ctx> DeclaredState<'ctx, '_> {
    fn encode(&'ctx self, ctx: &'ctx z3::Context) -> StateEncoding<'ctx> {
        let mut clauses: Vec<z3::ast::Bool> = Default::default();
        let children = ChildrenRelation(z3::FuncDecl::new(
            ctx,
            "preopen-children",
            &[
                &self.state.z3_file_type.sort,
                &z3::Sort::string(ctx),
                &self.state.z3_file_type.sort,
            ],
            &z3::Sort::bool(ctx),
        ));

        let any_dir = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_file_type.sort);
        let any_file = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_file_type.sort);
        let some_name = z3::ast::String::fresh_const(ctx, "");

        // Children relation.
        clauses.push(z3::ast::forall_const(
            ctx,
            &[&any_dir, &any_file, &some_name],
            &[],
            &z3::ast::Bool::or(
                ctx,
                self.children
                    .iter()
                    .map(|(dir, name, file)| {
                        z3::ast::Bool::and(
                            ctx,
                            &[
                                &dir.node._eq(&any_dir),
                                &file.node()._eq(&any_file),
                                &some_name._eq(&z3::ast::String::from_str(ctx, name).unwrap()),
                            ],
                        )
                    })
                    .collect_vec()
                    .as_slice(),
            )
            .ite(
                &children.has_child(ctx, &any_dir, &some_name, &any_file),
                &children
                    .has_child(ctx, &any_dir, &some_name, &any_file)
                    .not(),
            ),
        ));

        // File type.
        self.files
            .iter()
            .map(|file| match file {
                | EncodedFile::Directory(d) => {
                    clauses.push(
                        self.state.z3_file_type.variants[0]
                            .tester
                            .apply(&[d.node()])
                            .as_bool()
                            .unwrap(),
                    );
                },
                | EncodedFile::RegularFile(f) => {
                    clauses.push(
                        self.state.z3_file_type.variants[1]
                            .tester
                            .apply(&[f.node()])
                            .as_bool()
                            .unwrap(),
                    );
                },
            })
            .collect_vec();

        let any_segment = z3::ast::Dynamic::fresh_const(ctx, "", &self.state.z3_segment_type.sort);

        // clauses.push(z3::ast::forall_const(
        //     ctx,
        //     &[&any_segment],
        //     &[],
        //     &z3::ast::Bool::or(
        //         ctx,
        //         self.paths
        //             .iter()
        //             .flat_map(|(_param_name, path)| &path.segments)
        //             .map(|segment| any_segment._eq(&segment.node))
        //             .collect_vec()
        //             .as_slice(),
        //     ),
        // ));

        // Path segment components cannot be the empty string.
        clauses.push(z3::ast::Bool::and(
            ctx,
            &self
                .paths
                .iter()
                .map(|(_param_name, path_string)| {
                    path_string
                        .segments
                        .iter()
                        .map(|segment| {
                            z3::ast::Bool::and(
                                ctx,
                                &[self.state.z3_segment_type.variants[1]
                                    .tester
                                    .apply(&[&segment.node])
                                    .as_bool()
                                    .unwrap()],
                            )
                            .implies(&unsafe {
                                z3::ast::Int::wrap(
                                    ctx,
                                    z3_sys::Z3_mk_seq_length(
                                        ctx.get_z3_context(),
                                        self.state.z3_segment_type.variants[1].accessors[0]
                                            .apply(&[&segment.node])
                                            .as_string()
                                            .unwrap()
                                            .get_z3_ast(),
                                    ),
                                )
                                ._eq(&z3::ast::Int::from_u64(ctx, 0))
                                .not()
                            })
                        })
                        .collect_vec()
                })
                .flatten()
                .collect_vec(),
        ));

        // The first segment must be a component.
        clauses.push(z3::ast::Bool::and(
            ctx,
            &self
                .paths
                .iter()
                .map(|(_param_name, path)| {
                    self.state.z3_segment_type.variants[1]
                        .tester
                        .apply(&[&path.segments[0].node])
                        .as_bool()
                        .unwrap()
                })
                .collect_vec(),
        ));

        // Adjacent segments can't both be components.
        clauses.push(z3::ast::Bool::and(
            ctx,
            &self
                .paths
                .iter()
                .map(|(_param_name, path)| {
                    let mut subclauses = Vec::new();

                    for (i, segment) in path.segments.iter().enumerate() {
                        if i > 0 {
                            subclauses.push(
                                self.state.z3_segment_type.variants[1]
                                    .tester
                                    .apply(&[&segment.node])
                                    .as_bool()
                                    .unwrap()
                                    .implies(
                                        &self.state.z3_segment_type.variants[1]
                                            .tester
                                            .apply(&[&path.segments.get(i - 1).unwrap().node])
                                            .as_bool()
                                            .unwrap()
                                            .not(),
                                    ),
                            );
                        }
                    }

                    subclauses
                })
                .flatten()
                .collect_vec(),
        ));

        // Components cannot contain slash "/".
        clauses.push(z3::ast::Bool::and(
            ctx,
            self.paths
                .iter()
                .flat_map(|(_param_name, path)| &path.segments)
                .map(|segment| {
                    self.state.z3_segment_type.variants[1]
                        .tester
                        .apply(&[&segment.node])
                        .as_bool()
                        .unwrap()
                        .implies(
                            &self.state.z3_segment_type.variants[1].accessors[0]
                                .apply(&[&segment.node])
                                .as_string()
                                .unwrap()
                                .contains(&z3::ast::String::from_str(ctx, "/").unwrap())
                                .not(),
                        )
                })
                .collect_vec()
                .as_slice(),
        ));

        StateEncoding { clauses, children }
    }
}

#[derive(Debug)]
struct StateEncoding<'ctx> {
    clauses:  Vec<z3::ast::Bool<'ctx>>,
    children: ChildrenRelation<'ctx>,
}

#[derive(Debug)]
struct EncodedPreopenFs<'ctx> {
    root: EncodedDirectory<'ctx>,
}

#[derive(Debug)]
struct ChildrenRelation<'ctx>(z3::FuncDecl<'ctx>);

impl<'ctx> ChildrenRelation<'ctx> {
    fn has_child(
        &self,
        ctx: &'ctx z3::Context,
        dir: &dyn z3::ast::Ast<'ctx>,
        name: &dyn z3::ast::Ast<'ctx>,
        file: &dyn z3::ast::Ast<'ctx>,
    ) -> z3::ast::Bool<'ctx> {
        self.0.apply(&[dir, name, file]).as_bool().unwrap()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PreopenFsEncodingScope<'ctx> {
    files:    Vec<EncodedFile<'ctx>>,
    children: Vec<(EncodedDirectory<'ctx>, String, EncodedFile<'ctx>)>,
}

impl<'ctx> PreopenFsEncodingScope<'ctx> {
    fn new(ctx: &'ctx z3::Context, state: &State<'ctx>, preopen_idx: ResourceIdx) -> Self {
        Self {
            files:    Default::default(),
            children: Default::default(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum EncodedFile<'ctx> {
    Directory(EncodedDirectory<'ctx>),
    RegularFile(EncodedRegularFile<'ctx>),
}

impl<'ctx> EncodedFile<'ctx> {
    fn node(&self) -> &z3::ast::Dynamic {
        match self {
            | EncodedFile::Directory(encoded_directory) => &encoded_directory.node,
            | EncodedFile::RegularFile(f) => f.node(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct EncodedDirectory<'ctx> {
    node: z3::ast::Dynamic<'ctx>,
}

impl<'ctx> EncodedDirectory<'ctx> {
    fn node(&self) -> &z3::ast::Dynamic {
        &self.node
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct EncodedRegularFile<'ctx> {
    node: z3::ast::Dynamic<'ctx>,
}

impl<'ctx> EncodedRegularFile<'ctx> {
    fn node(&self) -> &z3::ast::Dynamic {
        &self.node
    }
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

#[derive(PartialEq, Eq, Clone, Debug)]
enum File {
    Directory(Directory),
    RegularFile(RegularFile),
}

impl File {
    fn encode<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        state: &State<'ctx>,
        scope: &mut PreopenFsEncodingScope<'ctx>,
    ) -> EncodedFile<'ctx> {
        match self {
            | File::Directory(directory) => {
                let dir = directory.encode(ctx, state, scope);

                EncodedFile::Directory(dir)
            },
            | File::RegularFile(regular_file) => {
                EncodedFile::RegularFile(regular_file.encode(ctx, state, scope))
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

#[derive(PartialEq, Eq, Clone, Debug)]
struct Directory {
    children: Vec<(OsString, File)>,
}

impl Directory {
    fn encode<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        state: &State<'ctx>,
        scope: &mut PreopenFsEncodingScope<'ctx>,
    ) -> EncodedDirectory<'ctx> {
        let node = z3::ast::Dynamic::fresh_const(ctx, "file--", &state.z3_file_type.sort);
        let enc_dir = EncodedDirectory { node };

        scope.files.push(EncodedFile::Directory(enc_dir.clone()));

        for (name, child) in &self.children {
            let child = child.encode(ctx, state, scope);

            scope.children.push((
                enc_dir.clone(),
                String::from_utf8(name.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
                child,
            ));
        }

        enc_dir
    }

    fn ingest(path: &Path) -> Result<Self, eyre::Error> {
        let mut paths: Vec<PathBuf> = Default::default();

        for entry in fs::read_dir(path).wrap_err("failed to read dir")? {
            let entry = entry?;

            paths.push(entry.path());
        }

        paths.sort();

        let children = paths
            .into_iter()
            .map(|path| -> Result<(OsString, File), eyre::Error> {
                let file = File::ingest(&path)?;

                Ok((path.file_name().unwrap().to_owned(), file))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { children })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct RegularFile {}

impl RegularFile {
    fn encode<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        state: &State<'ctx>,
        scope: &mut PreopenFsEncodingScope<'ctx>,
    ) -> EncodedRegularFile<'ctx> {
        let node = z3::ast::Dynamic::fresh_const(ctx, "file--", &state.z3_file_type.sort);
        let enc_file = EncodedRegularFile { node };

        scope.files.push(EncodedFile::RegularFile(enc_file.clone()));

        enc_file
    }

    fn ingest(_path: &Path) -> Result<Self, io::Error> {
        Ok(Self {})
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PathString {
    param_name: String,
    nsegments:  usize,
}

impl PathString {
    fn declare<'ctx>(
        &self,
        ctx: &'ctx z3::Context,
        state: &State<'ctx>,
    ) -> EncodedPathString<'ctx> {
        let mut segments = Vec::with_capacity(self.nsegments);

        for _i in 0..self.nsegments {
            segments.push(EncodedSegment {
                node: z3::ast::Dynamic::fresh_const(
                    ctx,
                    &format!("segment--{}--", self.param_name),
                    &state.z3_segment_type.sort,
                ),
            });
        }

        EncodedPathString { segments }
    }
}

#[derive(Debug)]
struct EncodedPathString<'ctx> {
    segments: Vec<EncodedSegment<'ctx>>,
}

#[derive(Debug)]
struct EncodedSegment<'ctx> {
    node: z3::ast::Dynamic<'ctx>,
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::resource::{Resource, Resources};

    #[test]
    fn ok() {
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let solver = z3::Solver::new(&ctx);
        let mut state = State::new(&ctx);
        let tempdir = tempdir().unwrap();

        fs::write(tempdir.path().join("file"), &[]).unwrap();
        fs::create_dir(tempdir.path().join("dir")).unwrap();
        fs::write(tempdir.path().join("dir").join("nested"), &[]).unwrap();

        let mut resources: Resources = Default::default();
        let dir_resource_idx = resources.push(Resource {
            state: WasiValue::Handle(3),
        });

        state.push_dir(dir_resource_idx, tempdir.path()).unwrap();
        state.push_path(
            "path".to_string(),
            PathString {
                param_name: "path".to_owned(),
                nsegments:  3,
            },
        );

        let declared_state = state.declare(&ctx);
        let state_encoding = declared_state.encode(&ctx);

        solver.push();
        solver.assert(&z3::ast::Bool::and(&ctx, &state_encoding.clauses));

        let encoded_preopen_fs = declared_state
            .preopens
            .get_by_key(&dir_resource_idx)
            .unwrap();

        assert_eq!(solver.check(), z3::SatResult::Sat);

        let model = solver.get_model().unwrap();
        let some_file = z3::ast::Dynamic::fresh_const(&ctx, "", &state.z3_file_type.sort);

        assert!(model
            .eval(
                &z3::ast::exists_const(
                    &ctx,
                    &[&some_file],
                    &[],
                    &state_encoding.children.has_child(
                        &ctx,
                        encoded_preopen_fs.root.node(),
                        &z3::ast::String::from_str(&ctx, "file").unwrap(),
                        &some_file,
                    ),
                ),
                true,
            )
            .unwrap()
            .simplify()
            .as_bool()
            .unwrap());
        assert!(!model
            .eval(
                &z3::ast::exists_const(
                    &ctx,
                    &[&some_file],
                    &[],
                    &state_encoding.children.has_child(
                        &ctx,
                        encoded_preopen_fs.root.node(),
                        &z3::ast::String::from_str(&ctx, "nonexistant").unwrap(),
                        &some_file,
                    ),
                ),
                true,
            )
            .unwrap()
            .simplify()
            .as_bool()
            .unwrap());
        let path = declared_state.paths.get("path").unwrap();

        // The second path segment cannot be a component because the first segment
        // is always a component.
        solver.push();
        solver.assert(
            &state.z3_segment_type.variants[1]
                .tester
                .apply(&[&path.segments[1].node])
                .as_bool()
                .unwrap(),
        );
        assert_eq!(solver.check(), z3::SatResult::Unsat);
        solver.pop(1);

        // Components cannot contain "/".
        solver.push();
        solver.assert(&z3::ast::Bool::and(
            &ctx,
            declared_state
                .paths
                .iter()
                .flat_map(|(_param_name, path)| &path.segments)
                .map(|segment| {
                    state.z3_segment_type.variants[1]
                        .tester
                        .apply(&[&segment.node])
                        .as_bool()
                        .unwrap()
                        .implies(
                            &state.z3_segment_type.variants[1].accessors[0]
                                .apply(&[&segment.node])
                                .as_string()
                                .unwrap()
                                .contains(&z3::ast::String::from_str(&ctx, "/").unwrap()),
                        )
                })
                .collect_vec()
                .as_slice(),
        ));
        assert_eq!(solver.check(), z3::SatResult::Unsat);
        solver.pop(1);

        panic!()
    }
}
