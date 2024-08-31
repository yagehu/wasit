use std::{collections::BTreeMap, fs, path::Path, vec::IntoIter};

use eyre::{Context as _, ContextCompat};
use z3::{
    ast::{self, forall_const, Ast},
    Context,
    DatatypeSort,
    FuncDecl,
    Solver,
    Sort,
};

use crate::preview1::spec::EncodedType;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FdType<'a, 'ctx>(&'a Sort<'ctx>);

impl<'ctx> FdType<'_, 'ctx> {
    pub fn sort(&self) -> &Sort {
        &self.0
    }

    pub fn fresh_const(&self, ctx: &'ctx Context) -> ast::Dynamic<'ctx> {
        ast::Dynamic::fresh_const(ctx, "fs--fd--", &self.0)
    }
}

impl<'a, 'ctx> From<&'a EncodedType<'ctx>> for FdType<'a, 'ctx> {
    fn from(value: &'a EncodedType<'ctx>) -> Self {
        Self(&value.datatype.sort)
    }
}

#[derive(Debug)]
pub struct FileType<'a, 'ctx>(&'a DatatypeSort<'ctx>);

impl<'a, 'ctx> From<&'a EncodedType<'ctx>> for FileType<'a, 'ctx> {
    fn from(value: &'a EncodedType<'ctx>) -> Self {
        Self(&value.datatype)
    }
}

impl<'ctx> FileType<'_, 'ctx> {
    pub fn sort(&self) -> &Sort {
        &self.0.sort
    }

    pub fn fresh_const(&self, ctx: &'ctx Context) -> ast::Dynamic<'ctx> {
        ast::Dynamic::fresh_const(ctx, "fs--file--", &self.0.sort)
    }

    pub fn r#type(&self, file: &dyn Ast<'ctx>) -> z3::ast::Dynamic<'ctx> {
        self.0.variants[0].accessors[0].apply(&[file])
    }
}

#[derive(Debug)]
struct FiletypeType<'a, 'ctx>(&'a DatatypeSort<'ctx>);

impl<'a, 'ctx> From<&'a EncodedType<'ctx>> for FiletypeType<'a, 'ctx> {
    fn from(value: &'a EncodedType<'ctx>) -> Self {
        Self(&value.datatype)
    }
}

impl<'ctx> FiletypeType<'_, 'ctx> {
    pub fn is_regular_file(&self, ast: &dyn z3::ast::Ast<'ctx>) -> z3::ast::Bool {
        self.0.variants[4].tester.apply(&[ast]).as_bool().unwrap()
    }
}

#[derive(Debug)]
pub struct DirEntryMapping<'ctx>(FuncDecl<'ctx>);

impl<'ctx> DirEntryMapping<'ctx> {
    pub fn new(ctx: &'ctx Context, file: &'ctx FileType<'_, 'ctx>) -> Self {
        Self(FuncDecl::new(
            ctx,
            "fs--dir-entry-mapping",
            &[file.sort(), &Sort::string(ctx), file.sort()],
            &Sort::bool(ctx),
        ))
    }

    pub fn exists(
        &self,
        dir: &dyn Ast<'ctx>,
        name: &ast::String<'ctx>,
        child: &dyn Ast<'ctx>,
    ) -> ast::Bool<'ctx> {
        self.0.apply(&[dir, name, child]).as_bool().unwrap()
    }
}

#[derive(Debug)]
pub struct FdFileMapping<'ctx>(FuncDecl<'ctx>);

impl<'ctx> FdFileMapping<'ctx> {
    pub fn new(
        ctx: &'ctx Context,
        fd: &'ctx FdType<'_, 'ctx>,
        file: &'ctx FileType<'_, 'ctx>,
    ) -> Self {
        Self(FuncDecl::new(
            ctx,
            "fs--fd-file-mapping",
            &[fd.sort(), file.sort()],
            &Sort::bool(ctx),
        ))
    }

    pub fn exists(&self, fd: &dyn Ast<'ctx>, file: &dyn Ast<'ctx>) -> ast::Bool<'ctx> {
        self.0.apply(&[fd, file]).as_bool().unwrap()
    }
}

#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub struct Entities<T>(Vec<T>);

impl<T> Entities<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, entity: T) -> usize {
        self.0.push(entity);
        self.0.len() - 1
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.0.get(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
        self.0.iter().enumerate()
    }
}

impl<T> IntoIterator for Entities<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

type FdId = usize;
type FileId = usize;

#[derive(Clone, Debug)]
pub struct WasiFs {
    fds:   Entities<()>,
    files: Entities<()>,

    dir_entry_mappings: BTreeMap<(FileId, String), FileId>,
    fd_file_mappings:   BTreeMap<FdId, FileId>,
}

impl WasiFs {
    pub fn new() -> Self {
        Self {
            fds:                Default::default(),
            files:              Default::default(),
            dir_entry_mappings: Default::default(),
            fd_file_mappings:   Default::default(),
        }
    }

    pub fn push_dir(&mut self, path: &Path) -> Result<FileId, eyre::Error> {
        let root_file_id = self.files.push(());
        let mut stack = vec![(root_file_id, path.to_path_buf())];

        while let Some((file_id, path)) = stack.pop() {
            let metadata = fs::metadata(&path)?;

            if metadata.file_type().is_dir() {
                let mut entries = fs::read_dir(&path)?.collect::<Result<Vec<_>, _>>()?;

                entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

                for entry in entries {
                    let child_file_id = self.files.push(());

                    if entry.file_type()?.is_dir() {
                        stack.push((child_file_id, entry.path()));
                    }

                    self.dir_entry_mappings.insert(
                        (
                            file_id,
                            String::from_utf8(entry.file_name().as_encoded_bytes().to_vec())
                                .wrap_err("file name is not UTF-8")?,
                        ),
                        child_file_id,
                    );
                }
            }
        }

        Ok(root_file_id)
    }

    pub fn register_fd(&mut self, base_file_id: FileId, path: &Path) -> Result<(), eyre::Error> {
        let mut curr_file_id = base_file_id;

        for component in path.components() {
            let component = String::from_utf8(component.as_os_str().as_encoded_bytes().to_vec())
                .wrap_err("invalid UTF-8 component")?;

            curr_file_id = *self
                .dir_entry_mappings
                .get(&(curr_file_id, component))
                .wrap_err("no child matching component")?;
        }

        let fd_id = self.fds.push(());

        self.fd_file_mappings.insert(fd_id, curr_file_id);

        Ok(())
    }
}

#[derive(Debug)]
pub struct WasiFsEncoding<'ctx> {
    fds:   BTreeMap<FdId, ast::Dynamic<'ctx>>,
    files: BTreeMap<FileId, ast::Dynamic<'ctx>>,

    clauses:           Vec<ast::Bool<'ctx>>,
    dir_entry_mapping: DirEntryMapping<'ctx>,
    fd_file_mapping:   FdFileMapping<'ctx>,
}

impl<'ctx> WasiFsEncoding<'ctx> {
    pub fn assert(&self, solver: &Solver) {
        self.clauses.iter().for_each(|clause| solver.assert(clause));
    }

    pub fn fd_maps_to_file(&self, fd: &dyn Ast<'ctx>, file: FileId) -> ast::Bool {
        self.fd_file_mapping
            .exists(fd, self.files.get(&file).unwrap())
    }
}

impl WasiFs {
    pub fn encode<'ctx>(
        &self,
        ctx: &'ctx Context,
        fd_type: &'ctx FdType,
        file_type: &'ctx FileType,
    ) -> WasiFsEncoding<'ctx> {
        let mut clauses = Vec::new();
        let fds = self
            .fds
            .iter()
            .map(|(id, _)| (id, fd_type.fresh_const(ctx)))
            .collect::<BTreeMap<_, _>>();
        let files = self
            .files
            .iter()
            .map(|(id, _)| (id, file_type.fresh_const(ctx)))
            .collect::<BTreeMap<_, _>>();

        if !fds.is_empty() {
            let any_fd = fd_type.fresh_const(ctx);

            clauses.push(forall_const(
                ctx,
                &[&any_fd],
                &[],
                &ast::Bool::or(
                    ctx,
                    fds.values()
                        .map(|fd| any_fd._eq(fd))
                        .collect::<Vec<_>>()
                        .as_slice(),
                ),
            ));
        }

        if !files.is_empty() {
            let some_file = file_type.fresh_const(ctx);

            clauses.push(forall_const(
                ctx,
                &[&some_file],
                &[],
                &ast::Bool::or(
                    ctx,
                    files
                        .values()
                        .map(|f| some_file._eq(f))
                        .collect::<Vec<_>>()
                        .as_slice(),
                ),
            ));
        }

        let dir_entry_mapping = DirEntryMapping::new(ctx, &file_type);

        if !self.dir_entry_mappings.is_empty() {
            let some_parent = file_type.fresh_const(ctx);
            let some_child = file_type.fresh_const(ctx);
            let some_name = ast::String::fresh_const(ctx, "fs--name--");

            clauses.push(forall_const(
                ctx,
                &[&some_parent, &some_name, &some_child],
                &[],
                &ast::Bool::or(
                    ctx,
                    self.dir_entry_mappings
                        .iter()
                        .map(|((parent, name), child)| {
                            ast::Bool::and(
                                ctx,
                                &[
                                    some_parent._eq(files.get(parent).unwrap()),
                                    some_name._eq(&ast::String::from_str(ctx, &name).unwrap()),
                                    some_child._eq(files.get(child).unwrap()),
                                ],
                            )
                        })
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .ite(
                    &dir_entry_mapping.exists(&some_parent, &some_name, &some_child),
                    &dir_entry_mapping
                        .exists(&some_parent, &some_name, &some_child)
                        .not(),
                ),
            ));
        }

        let fd_file_mapping = FdFileMapping::new(ctx, &fd_type, &file_type);

        if !self.fd_file_mappings.is_empty() {
            let some_fd = fd_type.fresh_const(ctx);
            let some_file = file_type.fresh_const(ctx);

            clauses.push(forall_const(
                ctx,
                &[&some_fd, &some_file],
                &[],
                &ast::Bool::or(
                    ctx,
                    self.fd_file_mappings
                        .iter()
                        .map(|(&fd, &file)| {
                            ast::Bool::and(
                                ctx,
                                &[
                                    some_fd._eq(fds.get(&fd).unwrap()),
                                    some_file._eq(files.get(&file).unwrap()),
                                ],
                            )
                        })
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
                .ite(
                    &fd_file_mapping.exists(&some_fd, &some_file),
                    &fd_file_mapping.exists(&some_fd, &some_file).not(),
                ),
            ));
        }

        WasiFsEncoding {
            fds,
            files,
            clauses,
            dir_entry_mapping,
            fd_file_mapping,
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use std::fs;

//     use tempfile::tempdir;
//     use z3::{Config, SatResult};

//     use super::*;

//     #[test]
//     fn ok() {
//         let cfg = Config::new();
//         let ctx = Context::new(&cfg);
//         let solver = Solver::new(&ctx);
//         let tempdir = tempdir().unwrap();

//         fs::create_dir_all(tempdir.path().join("d")).unwrap();
//         fs::write(tempdir.path().join("d").join("nested"), &[]).unwrap();
//         fs::write(tempdir.path().join("f"), &[]).unwrap();

//         let mut fs = WasiFs::new();
//         let root_dir = fs.push_dir(tempdir.path()).unwrap();

//         fs.register_fd(root_dir, Path::new("")).unwrap();

//         let fd_type = FdType::new(&ctx);
//         let file_type = FileType::new(&ctx);
//         let fs_encoding = fs.encode(&ctx, &fd_type, &file_type);

//         fs_encoding.assert(&solver);

//         let some_fd = fd_type.fresh_const(&ctx);

//         solver.assert(&fs_encoding.fd_maps_to_file(&some_fd, root_dir));

//         let result = solver.check();

//         assert_eq!(result, SatResult::Sat);
//     }
// }
