use arbitrary::Unstructured;
use eyre::Context as _;

use crate::{
    function_picker::FunctionPicker,
    preview1::spec::{Function, Interface, Spec},
    resource::Context,
    solve::FunctionScope,
    Environment,
};

#[derive(Clone, Copy, Debug)]
pub struct SolverPicker;

impl FunctionPicker for SolverPicker {
    fn pick_function<'i>(
        &self,
        u: &mut Unstructured,
        interface: &'i Interface,
        env: &Environment,
        ctx: &Context,
        spec: &Spec,
    ) -> Result<&'i Function, eyre::Error> {
        let mut candidates = Vec::new();

        for function in interface.functions.values() {
            let solver = z3::Solver::new(spec.ctx);
            let mut solver_params = z3::Params::new(spec.ctx);
            let random_seed: u32 = u.arbitrary()?;

            solver_params.set_bool("randomize", false);
            solver_params.set_u32("smt.random_seed", random_seed);
            solver.set_params(&solver_params);

            let scope = FunctionScope::new(spec, ctx, env, function);

            solver.push();

            if scope.solve_input_contract(&solver, u)?.is_some() {
                candidates.push(function);
            }

            solver.pop(1);
        }

        let function = *u
            .choose(&candidates)
            .wrap_err("solver function picker failed to choose from candidates")?;

        Ok(function)
    }
}
