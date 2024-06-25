use arbitrary::Unstructured;
use wazzi_specz_wasi::{Function, Interface, Spec};

use crate::{function_picker::FunctionPicker, resource::Context, Environment, FunctionScope};

#[derive(Clone, Copy, Debug)]
pub struct SolverPicker;

impl FunctionPicker for SolverPicker {
    fn pick_function<'i>(
        &self,
        u: &mut Unstructured,
        interface: &'i Interface,
        env: &Environment,
        ctx: &Context,
    ) -> Result<&'i Function, eyre::Error> {
        let mut candidates = Vec::new();

        for function in interface.functions.values() {
            let z3_cfg = z3::Config::new();
            let z3_ctx = z3::Context::new(&z3_cfg);
            let solver = z3::Solver::new(&z3_ctx);
            let mut solver_params = z3::Params::new(&z3_ctx);
            let random_seed: u32 = u.arbitrary()?;

            solver_params.set_bool("randomize", false);
            solver_params.set_u32("smt.random_seed", random_seed);
            solver.set_params(&solver_params);

            let scope = FunctionScope::new(&z3_ctx, solver, env, ctx, function);

            if scope.solve_input_contract(&z3_ctx, u)?.is_some() {
                candidates.push(function);
            }
        }

        let function = *u.choose(&candidates)?;

        Ok(function)
    }
}
