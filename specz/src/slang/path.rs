use z3::{
    ast::{self, forall_const, Ast},
    Context,
    DatatypeSort,
    FuncDecl,
    Solver,
    Sort,
};

use crate::preview1::spec::EncodedType;

#[derive(Debug)]
pub struct SegmentType<'a, 'ctx>(&'a DatatypeSort<'ctx>);

impl<'a, 'ctx> From<&'a EncodedType<'ctx>> for SegmentType<'a, 'ctx> {
    fn from(value: &'a EncodedType<'ctx>) -> Self {
        Self(&value.datatype)
    }
}

impl<'ctx> SegmentType<'_, 'ctx> {
    pub fn sort(&self) -> &Sort {
        &self.0.sort
    }

    pub fn fresh_const(&self, ctx: &'ctx Context) -> ast::Dynamic {
        ast::Dynamic::fresh_const(ctx, "path-param--segment--", &self.0.sort)
    }

    pub fn is_separator(&self, segment: &dyn Ast<'ctx>) -> ast::Bool<'ctx> {
        self.0.variants[0]
            .tester
            .apply(&[segment])
            .as_bool()
            .unwrap()
    }

    pub fn is_component(&self, segment: &dyn Ast<'ctx>) -> ast::Bool<'ctx> {
        self.0.variants[1]
            .tester
            .apply(&[segment])
            .as_bool()
            .unwrap()
    }

    pub fn component_string(&self, segment: &dyn Ast<'ctx>) -> z3::ast::String<'ctx> {
        self.0.variants[1].accessors[0]
            .apply(&[segment])
            .as_string()
            .unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct PathParam {
    name:       String,
    n_segments: usize,
}

impl PathParam {
    pub fn new(name: String, n_segments: usize) -> Self {
        Self { name, n_segments }
    }
}

impl PathParam {
    pub fn encode<'s, 'ctx>(
        &self,
        ctx: &'ctx Context,
        segment_type: &'s SegmentType<'_, 'ctx>,
    ) -> PathParamEncoding<'s, ast::Dynamic<'ctx>>
    where
        's: 'ctx,
    {
        let mut clauses = Vec::new();
        let mut segments: Vec<ast::Dynamic> = Vec::with_capacity(self.n_segments);
        let mut component_idxs = Vec::new();

        let component_idx_mapping = FuncDecl::new(
            ctx,
            format!("{}--component-idx-map", self.name),
            &[segment_type.sort(), &Sort::int(ctx)],
            &Sort::bool(ctx),
        );

        for i in 0..self.n_segments {
            let segment = segment_type.fresh_const(ctx);

            clauses.push(
                segment_type.is_component(&segment).implies(
                    &segment_type
                        .component_string(&segment)
                        .contains(&z3::ast::String::from_str(ctx, "/").unwrap())
                        .not(),
                ),
            );

            // The first segment must be a component.
            if i == 0 {
                clauses.push(segment_type.is_component(&segment));
            }

            // Adjacent segments can't both be components.
            if i > 0 {
                if let Some(prev_segment) = segments.get(i - 1) {
                    clauses.push(
                        segment_type
                            .is_component(&segment)
                            .implies(&segment_type.is_component(prev_segment).not()),
                    );
                }
            }

            let mut component_idx = ast::Int::from_u64(ctx, 0);

            for j in 0..i {
                let prev_segment = segments.get(j).unwrap();
                let idx = ast::Int::fresh_const(ctx, "path--");

                clauses.push(segment_type.is_component(prev_segment).ite(
                    &idx._eq(&ast::Int::add(
                        ctx,
                        &[&component_idx, &ast::Int::from_u64(ctx, 1)],
                    )),
                    &idx._eq(&component_idx),
                ));

                component_idx = idx;
            }

            component_idxs.push((segment.clone(), component_idx));
            segments.push(segment);
        }

        let some_segment = segment_type.fresh_const(ctx);
        let some_int = ast::Int::fresh_const(ctx, "path--");

        clauses.push(forall_const(
            ctx,
            &[&some_segment, &some_int],
            &[],
            &ast::Bool::or(
                ctx,
                &component_idxs
                    .iter()
                    .map(|(segment, idx)| {
                        ast::Bool::and(
                            ctx,
                            &[
                                segment_type.is_component(segment),
                                some_segment._eq(segment),
                                some_int._eq(&idx),
                            ],
                        )
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .ite(
                &component_idx_mapping
                    .apply(&[&some_segment, &some_int])
                    .as_bool()
                    .unwrap(),
                &component_idx_mapping
                    .apply(&[&some_segment, &some_int])
                    .as_bool()
                    .unwrap()
                    .not(),
            ),
        ));

        PathParamEncoding {
            clauses,
            segments,
            component_idx_mapping,
        }
    }
}

#[derive(Debug)]
pub struct PathParamEncoding<'ctx, T> {
    clauses:               Vec<ast::Bool<'ctx>>,
    segments:              Vec<T>,
    component_idx_mapping: FuncDecl<'ctx>,
}

impl<'ctx, T> PathParamEncoding<'ctx, T> {
    pub fn assert(&self, solver: &Solver) {
        self.clauses.iter().for_each(|clause| {
            solver.assert(clause);
        });
    }

    pub fn segments(&self) -> impl Iterator<Item = &T> {
        self.segments.iter()
    }

    pub fn component_idx_mapping(&self) -> &FuncDecl {
        &self.component_idx_mapping
    }
}

// #[cfg(test)]
// mod tests {
//     use z3::{Config, SatResult, Solver};

//     use super::*;

//     #[test]
//     fn ok() {
//         let cfg = Config::new();
//         let ctx = Context::new(&cfg);
//         let solver = Solver::new(&ctx);
//         let path_param = PathParam::new(6);
//         let segment_type = SegmentType::new(&ctx);
//         let encoding = path_param.encode(&ctx, &segment_type);
//         let segments = encoding.segments().collect::<Vec<_>>();

//         encoding.assert(&solver);
//         solver.assert(&segment_type.is_separator(*segments.get(1).unwrap()));
//         solver.assert(&segment_type.is_component(*segments.get(2).unwrap()));

//         assert!(solver.check() == SatResult::Sat);

//         let model = solver.get_model().unwrap();
//         let some_int = ast::Int::fresh_const(&ctx, "");

//         assert!(model
//             .eval(
//                 &encoding
//                     .component_idx_mapping()
//                     .apply(&[*segments.first().unwrap(), &ast::Int::from_u64(&ctx, 0)]),
//                 false,
//             )
//             .unwrap()
//             .as_bool()
//             .unwrap()
//             .as_bool()
//             .unwrap());
//         assert!(model
//             .eval(
//                 &forall_const(
//                     &ctx,
//                     &[&some_int],
//                     &[],
//                     &encoding
//                         .component_idx_mapping()
//                         .apply(&[*segments.get(1).unwrap(), &some_int])
//                         .as_bool()
//                         .unwrap()
//                         .not(),
//                 ),
//                 false,
//             )
//             .unwrap()
//             .as_bool()
//             .unwrap());
//         assert!(model
//             .eval(
//                 &encoding
//                     .component_idx_mapping()
//                     .apply(&[*segments.get(2).unwrap(), &ast::Int::from_u64(&ctx, 1)]),
//                 false,
//             )
//             .unwrap()
//             .as_bool()
//             .unwrap()
//             .as_bool()
//             .unwrap());
//     }

//     #[test]
//     fn component_cant_contain_slashes() {
//         let cfg = Config::new();
//         let ctx = Context::new(&cfg);
//         let solver = Solver::new(&ctx);
//         let path = PathParam::new(3);
//         let segment_type = SegmentType::new(&ctx);
//         let encoding = path.encode(&ctx, &segment_type);

//         encoding.assert(&solver);
//         solver.assert(
//             &segment_type
//                 .component_string(encoding.segments().next().unwrap())
//                 ._eq(&z3::ast::String::from_str(&ctx, "/").unwrap()),
//         );

//         assert_eq!(solver.check(), SatResult::Unsat);
//     }
// }
