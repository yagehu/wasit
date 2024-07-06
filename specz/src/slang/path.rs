use z3::{
    ast::{self, forall_const, Ast, Bool, Datatype, Int},
    Context,
    DatatypeAccessor,
    DatatypeBuilder,
    DatatypeSort,
    FuncDecl,
    Solver,
    Sort,
};

#[derive(Debug)]
pub struct PathEncoder<'ctx, 's> {
    solver: &'s Solver<'ctx>,

    segment_datatype: DatatypeSort<'ctx>,

    /// Maps path components to their indices.
    pub component_idx_map: FuncDecl<'ctx>,
}

impl<'ctx, 's> PathEncoder<'ctx, 's> {
    pub fn new(ctx: &'ctx Context, solver: &'s Solver<'ctx>) -> Self {
        let segment_datatype = DatatypeBuilder::new(ctx, "segment")
            .variant("separator", vec![])
            .variant(
                "component",
                vec![("string", DatatypeAccessor::Sort(Sort::string(ctx)))],
            )
            .finish();
        let component_idx_map = FuncDecl::new(
            ctx,
            "component-idx-map",
            &[&segment_datatype.sort, &Sort::int(ctx)],
            &Sort::bool(ctx),
        );

        Self {
            solver,
            segment_datatype,
            component_idx_map,
        }
    }

    pub fn component_string(&self, segment: &dyn Ast<'ctx>) -> z3::ast::String<'ctx> {
        self.segment_datatype.variants[1].accessors[0]
            .apply(&[segment])
            .as_string()
            .unwrap()
    }

    pub fn encode_path(&self, ctx: &'ctx Context, num_segments: usize) -> Vec<Datatype> {
        let mut segments: Vec<Datatype> = Vec::with_capacity(num_segments);
        let mut component_idx_mappings = Vec::new();

        for i in 0..num_segments {
            let segment =
                Datatype::fresh_const(ctx, &format!("path--{i}"), &self.segment_datatype.sort);

            self.solver.assert(
                &self.segment_is_component(&segment).implies(
                    &self
                        .component_string(&segment)
                        .contains(&z3::ast::String::from_str(ctx, "/").unwrap())
                        .not(),
                ),
            );

            // The first segment must be a component.
            if i == 0 {
                self.solver.assert(&self.segment_is_component(&segment));
            }

            // Adjacent segments can't both be components.
            if i > 0 {
                if let Some(prev_segment) = segments.get(i - 1) {
                    self.solver.assert(
                        &self
                            .segment_is_component(&segment)
                            .implies(&self.segment_is_component(prev_segment).not()),
                    );
                }
            }

            let mut component_idx = Int::from_u64(ctx, 0);

            for j in 0..i {
                let prev_segment = segments.get(j).unwrap();
                let idx = Int::fresh_const(ctx, "path--");

                self.solver
                    .assert(&self.segment_is_component(prev_segment).ite(
                        &idx._eq(&Int::add(ctx, &[&component_idx, &Int::from_u64(ctx, 1)])),
                        &idx._eq(&component_idx),
                    ));

                component_idx = idx;
            }

            component_idx_mappings.push((segment.clone(), component_idx));
            segments.push(segment);
        }

        let mut clauses = Vec::with_capacity(component_idx_mappings.len());
        let some_segment = Datatype::fresh_const(ctx, "path--", &self.segment_datatype.sort);
        let some_int = Int::fresh_const(ctx, "path--");

        for (component, idx) in component_idx_mappings {
            clauses.push(Bool::and(
                ctx,
                &[
                    self.segment_is_component(&component),
                    some_segment._eq(&component),
                    some_int._eq(&idx),
                ],
            ));
        }

        self.solver.assert(&forall_const(
            ctx,
            &[&some_segment, &some_int],
            &[],
            &Bool::or(ctx, &clauses).ite(
                &self
                    .component_idx_map
                    .apply(&[&some_segment, &some_int])
                    .as_bool()
                    .unwrap(),
                &self
                    .component_idx_map
                    .apply(&[&some_segment, &some_int])
                    .as_bool()
                    .unwrap()
                    .not(),
            ),
        ));

        segments
    }
}

#[derive(Debug)]
pub struct SegmentType<'ctx>(DatatypeSort<'ctx>);

impl<'ctx> SegmentType<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Self {
        Self(
            DatatypeBuilder::new(ctx, "segment")
                .variant("separator", vec![])
                .variant(
                    "component",
                    vec![("string", DatatypeAccessor::Sort(Sort::string(ctx)))],
                )
                .finish(),
        )
    }

    pub fn sort(&self) -> &Sort {
        &self.0.sort
    }

    pub fn fresh_const(&self, ctx: &'ctx Context) -> impl Ast {
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
}

#[derive(Clone, Debug)]
pub struct PathParam {
    n_components: usize,
}

impl PathParam {
    pub fn encode(&self) -> PathParamEncoding {
    }
}

#[derive(Clone, Debug)]
pub struct PathParamEncoding {}

#[cfg(test)]
mod tests {
    use z3::{Config, SatResult};

    use super::*;

    #[test]
    fn ok() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let encoder = PathEncoder::new(&ctx, &solver);
        let segments = encoder.encode_path(&ctx, 3);

        solver.assert(&encoder.segment_is_separator(segments.get(1).unwrap()));
        solver.assert(&encoder.segment_is_component(segments.get(2).unwrap()));

        assert!(solver.check() == SatResult::Sat);

        let model = solver.get_model().unwrap();
        let some_int = Int::fresh_const(&ctx, "");

        assert!(model
            .eval(
                &encoder
                    .component_idx_map
                    .apply(&[segments.first().unwrap(), &Int::from_u64(&ctx, 0)]),
                false,
            )
            .unwrap()
            .as_bool()
            .unwrap()
            .as_bool()
            .unwrap());
        assert!(model
            .eval(
                &forall_const(
                    &ctx,
                    &[&some_int],
                    &[],
                    &encoder
                        .component_idx_map
                        .apply(&[segments.get(1).unwrap(), &some_int])
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
                false,
            )
            .unwrap()
            .as_bool()
            .unwrap());
        assert!(model
            .eval(
                &encoder
                    .component_idx_map
                    .apply(&[segments.get(2).unwrap(), &Int::from_u64(&ctx, 1)]),
                false,
            )
            .unwrap()
            .as_bool()
            .unwrap()
            .as_bool()
            .unwrap());
    }

    #[test]
    fn component_cant_contain_slashes() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let encoder = PathEncoder::new(&ctx, &solver);
        let segments = encoder.encode_path(&ctx, 3);

        solver.assert(
            &encoder
                .component_string(segments.first().unwrap())
                ._eq(&z3::ast::String::from_str(&ctx, "/").unwrap()),
        );

        assert!(solver.check() == SatResult::Unsat);
    }
}
