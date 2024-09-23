use arbitrary::Unstructured;
use eyre::Context;
use itertools::Itertools;

use super::CallStrategy;
use crate::{
    spec::{Function, Spec, WasiValue},
    Environment,
    ResourceIdx,
    RuntimeContext,
};

pub struct StatelessStrategy<'u, 'data, 'env, 'ctx> {
    u:   &'u mut Unstructured<'data>,
    env: &'env Environment,
    ctx: &'ctx RuntimeContext,
}

impl<'u, 'data, 'env, 'ctx> StatelessStrategy<'u, 'data, 'env, 'ctx> {
    pub fn new(
        u: &'u mut Unstructured<'data>,
        env: &'env Environment,
        ctx: &'ctx RuntimeContext,
    ) -> Self {
        Self { u, env, ctx }
    }
}

impl CallStrategy for StatelessStrategy<'_, '_, '_, '_> {
    fn select_function<'spec>(
        &mut self,
        spec: &'spec Spec,
    ) -> Result<&'spec Function, eyre::Error> {
        let mut pool: Vec<&Function> = Default::default();

        for (_interface_name, interface) in spec.interfaces.iter() {
            for (_function_name, function) in &interface.functions {
                let mut is_candidate = true;

                for param in function.params.iter() {
                    let tdef = param.tref.resolve(spec);

                    if tdef.state.is_none() {
                        continue;
                    }

                    let resources = match self.env.resources_by_types.get(&param.name) {
                        | None => {
                            is_candidate = false;
                            break;
                        },
                        | Some(resources) => resources,
                    };

                    if resources.is_empty() {
                        is_candidate = false;
                        break;
                    }
                }

                if is_candidate {
                    pool.push(function);
                }
            }
        }

        Ok(self
            .u
            .choose(pool.as_slice())
            .wrap_err("failed to choose a function")?)
    }

    #[tracing::instrument(skip(self, spec))]
    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<(WasiValue, Option<ResourceIdx>)>, eyre::Error> {
        let mut params = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            let tdef = param.tref.resolve(spec);

            match &tdef.state {
                | None => {
                    params.push((tdef.wasi.arbitrary_value(spec, self.u)?, None));
                },
                | Some(_state_type) => {
                    let resources = self
                        .env
                        .resources_by_types
                        .get(&tdef.name)
                        .unwrap()
                        .iter()
                        .cloned()
                        .collect_vec();
                    let resource_id = *self
                        .u
                        .choose(&resources)
                        .wrap_err("failed to choose a resource")?;
                    let resource = self.ctx.resources.get(&resource_id).unwrap();

                    params.push((resource.to_owned(), Some(resource_id)));
                },
            }
        }

        Ok(params)
    }
}

#[cfg(test)]
mod tests {
    use arbitrary::Unstructured;

    use super::*;

    #[test]
    fn object_safe() {
        let data = vec![];
        let mut u = Unstructured::new(&data);
        let env = Environment::new();
        let ctx = RuntimeContext::new();
        let mut strat = StatelessStrategy {
            u:   &mut u,
            env: &env,
            ctx: &ctx,
        };
        let strat: &mut dyn CallStrategy = &mut strat;
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let spec = Spec::preview1(&ctx).unwrap();

        assert!(strat.select_function(&spec).is_err());
    }
}
