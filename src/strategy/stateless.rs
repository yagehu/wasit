use arbitrary::Unstructured;

use super::CallStrategy;
use crate::{
    spec::{Function, Spec},
    Environment,
    ValueMeta,
};

pub struct StatelessStrategy<'u, 'data, 'env> {
    u:   &'u mut Unstructured<'data>,
    env: &'env Environment,
}

impl<'u, 'data, 'env> StatelessStrategy<'u, 'data, 'env> {
    pub fn new(u: &'u mut Unstructured<'data>, env: &'env Environment) -> Self {
        Self { u, env }
    }
}

impl CallStrategy for StatelessStrategy<'_, '_, '_> {
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

                    if tdef.attributes.is_none() {
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

        Ok(self.u.choose(pool.as_slice())?)
    }

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<ValueMeta>, eyre::Error> {
        let mut params = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            let tdef = param.tref.resolve(spec);

            match tdef.attributes {
                | None => {
                    params.push(ValueMeta {
                        wasi:     tdef.wasi.arbitrary_value(spec, self.u)?,
                        resource: None,
                    });
                },
                | Some(_) => todo!(),
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
        let mut strat = StatelessStrategy {
            u:   &mut u,
            env: &env,
        };
        let strat: &mut dyn CallStrategy = &mut strat;
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let spec = Spec::preview1(&ctx).unwrap();

        assert!(strat.select_function(&spec).is_err());
    }
}
