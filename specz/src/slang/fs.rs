use z3::{
    ast::{forall_const, Ast, Bool, Datatype, Dynamic},
    Context,
    DatatypeSort,
    FuncDecl,
    Solver,
    Sort,
};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum File {
    RegularFile(RegularFile),
    Directory(Directory),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RegularFile {}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Directory {
    pub entries: Vec<DirEntry>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub file: File,
}

#[derive(Debug)]
pub struct FsEncoder<'ctx, 'd, 's> {
    solver: &'s Solver<'ctx>,

    fd_sort:       &'d Sort<'ctx>,
    file_datatype: &'d DatatypeSort<'ctx>,

    /// Maps directories to the entries they contain through their filenames.
    pub entries_map: FuncDecl<'ctx>,

    /// Maps fds to files.
    pub fd_map: FuncDecl<'ctx>,
}

impl<'ctx, 'd, 's> FsEncoder<'ctx, 'd, 's>
where
    'd: 'ctx,
    's: 'ctx,
{
    pub fn new(
        ctx: &'ctx Context,
        solver: &'s Solver<'ctx>,
        fd: &'d Sort<'ctx>,
        file: &'d DatatypeSort<'ctx>,
    ) -> Self {
        let entries_map = FuncDecl::new(
            ctx,
            "entries-map",
            &[&file.sort, &Sort::string(ctx), &file.sort],
            &Sort::bool(ctx),
        );
        let fd_map = FuncDecl::new(ctx, "fd-map", &[fd], &file.sort);

        Self {
            solver,
            fd_sort: fd,
            file_datatype: file,
            entries_map,
            fd_map,
        }
    }

    pub fn encode_file(
        &self,
        ctx: &'ctx Context,
        file: &File,
        fd: &Dynamic<'ctx>,
    ) -> Datatype<'ctx> {
        let mut dir_entries = Vec::new();
        let root = self.encode_file_helper(ctx, file, &mut dir_entries);
        let fd_mappings = vec![(fd, &root)];

        self.encode_entries_map(ctx, dir_entries);
        self.encode_fd_map(ctx, fd_mappings);

        root
    }

    fn encode_file_helper(
        &self,
        ctx: &'ctx Context,
        file: &File,
        dir_entries: &mut Vec<(Datatype<'ctx>, z3::ast::String<'ctx>, Datatype<'ctx>)>,
    ) -> Datatype<'ctx> {
        let f = Datatype::fresh_const(ctx, "fs--file--", &self.file_datatype.sort);

        match file {
            | File::RegularFile(_) => {
                self.solver.assert(
                    &self.file_datatype.variants[0]
                        .tester
                        .apply(&[&f])
                        .as_bool()
                        .unwrap(),
                );
            },
            | File::Directory(directory) => {
                self.solver.assert(
                    &self.file_datatype.variants[1]
                        .tester
                        .apply(&[&f])
                        .as_bool()
                        .unwrap(),
                );

                for entry in directory.entries.iter() {
                    let child = self.encode_file_helper(ctx, &entry.file, dir_entries);
                    let filename = z3::ast::String::from_str(ctx, &entry.name).unwrap();

                    dir_entries.push((f.clone(), filename, child));
                }
            },
        }

        f
    }

    fn encode_entries_map(
        &self,
        ctx: &'ctx Context,
        dir_entries: Vec<(Datatype, z3::ast::String, Datatype)>,
    ) {
        let some_dir = Datatype::fresh_const(ctx, "fs--", &self.file_datatype.sort);
        let some_file = Datatype::fresh_const(ctx, "fs--", &self.file_datatype.sort);
        let some_filename = z3::ast::String::fresh_const(ctx, "fs--");
        let mut clauses = Vec::with_capacity(dir_entries.len());

        for (dir, filename, file) in dir_entries.iter() {
            clauses.push(Bool::and(
                ctx,
                &[
                    some_dir._eq(dir),
                    some_filename._eq(filename),
                    some_file._eq(file),
                ],
            ));
        }

        self.solver.assert(&forall_const(
            ctx,
            &[&some_dir, &some_filename, &some_file],
            &[],
            &Bool::or(ctx, &clauses).ite(
                &self
                    .entries_map
                    .apply(&[&some_dir, &some_filename, &some_file])
                    .as_bool()
                    .unwrap(),
                &self
                    .entries_map
                    .apply(&[&some_dir, &some_filename, &some_file])
                    .as_bool()
                    .unwrap()
                    .not(),
            ),
        ));
    }

    fn encode_fd_map(
        &self,
        ctx: &'ctx Context,
        fd_mappings: Vec<(&Dynamic<'ctx>, &Datatype<'ctx>)>,
    ) {
        let some_fd = Dynamic::fresh_const(ctx, "fs--", &self.fd_sort);
        let some_file = Datatype::fresh_const(ctx, "fs--", &self.file_datatype.sort);
        let mut clauses = Vec::with_capacity(fd_mappings.len());

        for (fd, file) in fd_mappings.iter() {
            clauses.push(Bool::and(ctx, &[some_fd._eq(fd), some_file._eq(file)]));
        }

        self.solver.assert(&forall_const(
            ctx,
            &[&some_fd, &some_file],
            &[],
            &Bool::or(ctx, &clauses).ite(
                &self
                    .fd_map
                    .apply(&[&some_fd])
                    .as_datatype()
                    .unwrap()
                    ._eq(&some_file),
                &self
                    .fd_map
                    .apply(&[&some_fd])
                    .as_datatype()
                    .unwrap()
                    ._eq(&some_file)
                    .not(),
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use z3::{
        ast::{self, exists_const, Dynamic},
        Config,
        DatatypeBuilder,
        SatResult,
    };

    use super::*;

    #[test]
    fn ok() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let fd_sort = Sort::uninterpreted(&ctx, z3::Symbol::String("fd".to_string()));
        let file_datatype = DatatypeBuilder::new(&ctx, "file")
            .variant("regular-file", vec![])
            .variant("directory", vec![])
            .finish();
        let fd = Dynamic::new_const(&ctx, "fd", &fd_sort);
        let encoder = FsEncoder::new(&ctx, &solver, &fd_sort, &file_datatype);
        let root = encoder.encode_file(
            &ctx,
            &File::Directory(Directory {
                entries: vec![
                    DirEntry {
                        name: "f".to_owned(),
                        file: File::RegularFile(RegularFile {}),
                    },
                    DirEntry {
                        name: "d".to_owned(),
                        file: File::Directory(Directory { entries: vec![] }),
                    },
                ],
            }),
            &fd,
        );
        let some_file_f = Datatype::fresh_const(&ctx, "", &file_datatype.sort);
        let some_file_d = Datatype::fresh_const(&ctx, "", &file_datatype.sort);
        let result = solver.check();

        assert_eq!(result, SatResult::Sat);

        let model = solver.get_model().unwrap();

        assert!(model
            .eval(
                &exists_const(
                    &ctx,
                    &[&some_file_f, &some_file_d],
                    &[],
                    &Bool::and(
                        &ctx,
                        &[
                            some_file_f._eq(&some_file_d).not(),
                            encoder
                                .entries_map
                                .apply(&[
                                    &root,
                                    &ast::String::from_str(&ctx, "f").unwrap(),
                                    &some_file_f,
                                ])
                                .as_bool()
                                .unwrap(),
                            encoder
                                .entries_map
                                .apply(&[
                                    &root,
                                    &ast::String::from_str(&ctx, "d").unwrap(),
                                    &some_file_d,
                                ])
                                .as_bool()
                                .unwrap(),
                        ],
                    ),
                ),
                false,
            )
            .unwrap()
            .as_bool()
            .unwrap());
    }
}
