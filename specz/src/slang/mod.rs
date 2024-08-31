use z3::{
    ast::{self, Ast},
    Context,
    DatatypeSort,
    Sort,
};

use crate::preview1::spec::EncodedType;

pub mod fs;
pub mod path;

#[derive(Debug)]
pub struct OptionType<'a, 'ctx>(&'a DatatypeSort<'ctx>);

impl<'a, 'ctx> From<&'a EncodedType<'ctx>> for OptionType<'a, 'ctx> {
    fn from(value: &'a EncodedType<'ctx>) -> Self {
        Self(&value.datatype)
    }
}

impl<'ctx> OptionType<'_, 'ctx> {
    pub fn sort(&self) -> &Sort {
        &self.0.sort
    }

    pub fn is_none(&self, x: &dyn Ast<'ctx>) -> ast::Bool {
        self.0.variants[0].tester.apply(&[x]).as_bool().unwrap()
    }

    pub fn is_some(&self, x: &dyn Ast<'ctx>) -> ast::Bool {
        self.0.variants[1].tester.apply(&[x]).as_bool().unwrap()
    }

    pub fn inner(&self, x: &dyn Ast<'ctx>) -> ast::Dynamic {
        self.0.variants[1].accessors[0].apply(&[x])
    }

    pub fn fresh_const(&self, ctx: &'ctx Context) -> ast::Dynamic {
        ast::Dynamic::fresh_const(ctx, "", &self.0.sort)
    }
}

// #[cfg(test)]
// mod tests {
//     use std::{fs, path::Path};

//     use tempfile::tempdir;
//     use z3::{
//         ast::{self, exists_const},
//         Config,
//         Context,
//         SatResult,
//         Solver,
//     };

//     use super::{
//         fs::{FdType, FileType, WasiFs},
//         path::{PathParam, SegmentType},
//         *,
//     };

//     #[test]
//     fn ok() {
//         let cfg = Config::new();
//         let ctx = Context::new(&cfg);
//         let solver = Solver::new(&ctx);
//         let mut wasi_fs = WasiFs::new();
//         let num_components = 3;
//         let path = PathParam::new(num_components);
//         let tempdir = tempdir().unwrap();
//         let fd_type = FdType::new(&ctx);
//         let file_type = FileType::new(&ctx);
//         let segment_type = SegmentType::new(&ctx);
//         let option_file = OptionType::new(&ctx, file_type.sort());
//         let option_segment = OptionType::new(&ctx, segment_type.sort());
//         let root_dir = wasi_fs.push_dir(tempdir.path()).unwrap();

//         fs::create_dir_all(tempdir.path().join("d")).unwrap();
//         fs::write(tempdir.path().join("f"), &[]).unwrap();
//         wasi_fs.register_fd(root_dir, Path::new("")).unwrap();

//         let param_fd = fd_type.fresh_const(&ctx);
//         let fs_encoding = wasi_fs.encode(&ctx, &fd_type, &file_type);
//         let path_encoding = path.encode(&ctx, &segment_type);
//         let mut curr_component_file = option_file.fresh_const(&ctx);

//         solver.assert(&ast::Bool::and(
//             &ctx,
//             &[
//                 option_file.is_some(&curr_component_file),
//                 fs_encoding.fd_maps_to_file(&param_fd, root_dir),
//             ],
//         ));

//         for segment in path_encoding.segments() {
//             let next_component = ast::Dynamic::fresh_const(&ctx, "", &option_segment.sort());
//             let some_component_idx = ast::Int::fresh_const(&ctx, "");

//             solver.assert(
//                 &ast::Bool::and(
//                     &ctx,
//                     &[
//                         segment_type.is_component(segment),
//                         path_encoding
//                             .component_idx_mapping()
//                             .apply(&[
//                                 segment,
//                                 &ast::Int::sub(
//                                     &ctx,
//                                     &[ast::Int::from_u64(&ctx, num_components as u64)],
//                                 ),
//                             ])
//                             .as_bool()
//                             .unwrap()
//                             .not(),
//                     ],
//                 )
//                 .ite(
//                     &ast::Bool::and(
//                         &ctx,
//                         &[
//                             option_segment.is_some(&next_component),
//                             segment_type.is_component(&option_segment.inner(&next_component)),
//                             ast::Bool::or(
//                                 &ctx,
//                                 path_encoding
//                                     .segments()
//                                     .map(|seg| seg._eq(&option_segment.inner(&next_component)))
//                                     .collect::<Vec<_>>()
//                                     .as_slice(),
//                             ),
//                         ],
//                     ),
//                     &option_segment.is_none(&next_component),
//                 ),
//             );
//             solver.assert(
//                 &ast::Bool::and(
//                     &ctx,
//                     &[
//                         segment_type.is_component(segment),
//                         segment_type.is_component(&option_segment.inner(&next_component)),
//                         option_segment.is_some(&next_component),
//                     ],
//                 )
//                 .implies(&exists_const(
//                     &ctx,
//                     &[&some_component_idx],
//                     &[],
//                     &ast::Bool::and(
//                         &ctx,
//                         &[
//                             &path_encoding
//                                 .component_idx_mapping()
//                                 .apply(&[segment, &some_component_idx])
//                                 .as_bool()
//                                 .unwrap(),
//                             &path_encoding
//                                 .component_idx_mapping()
//                                 .apply(&[
//                                     &option_segment.inner(&next_component),
//                                     &ast::Int::add(
//                                         &ctx,
//                                         &[&some_component_idx, &ast::Int::from_i64(&ctx, 1)],
//                                     ),
//                                 ])
//                                 .as_bool()
//                                 .unwrap(),
//                         ],
//                     ),
//                 )),
//             );
//         }

//         assert_eq!(solver.check(), SatResult::Sat);
//     }
// }
