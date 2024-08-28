use arbitrary::Unstructured;
use eyre::ContextCompat as _;

use crate::{
    param_generator::ParamsGenerator,
    preview1::spec::{Function, Spec},
    resource::Context,
    solve::FunctionScope,
    Environment,
    Value,
};

#[derive(Clone, Copy, Debug)]
pub struct StatefulParamsGenerator;

impl ParamsGenerator for StatefulParamsGenerator {
    fn generate_params(
        &self,
        u: &mut Unstructured,
        env: &Environment,
        ctx: &Context,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<Value>, eyre::Error> {
        let solver = z3::Solver::new(spec.ctx);
        let random_seed: u32 = u.arbitrary()?;
        let mut solver_params = z3::Params::new(spec.ctx);

        solver_params.set_bool("randomize", false);
        solver_params.set_u32("smt.random_seed", random_seed);
        solver.set_params(&solver_params);

        let function_scope = FunctionScope::new(spec, ctx, env, function);
        let params = function_scope
            .solve_input_contract(&solver, u)?
            .wrap_err("no solution found")?;

        Ok(params)
    }
}
