use arbitrary::Unstructured;
use eyre::Context;
use itertools::Itertools;

use super::CallStrategy;
use crate::{
    resource::HighLevelValue,
    spec::{Function, Spec, WasiValue},
    Environment,
    ResourceIdx,
};

pub struct StatelessStrategy<'u, 'data> {
    u: &'u mut Unstructured<'data>,
}

impl<'u, 'data> StatelessStrategy<'u, 'data> {
    pub fn new(u: &'u mut Unstructured<'data>) -> Self {
        Self { u }
    }
}

impl CallStrategy for StatelessStrategy<'_, '_> {
    fn select_function<'spec>(
        &mut self,
        spec: &'spec Spec,
        env: &Environment,
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

                    let resources = match env.resources_by_types.get(&param.name) {
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
        env: &Environment,
    ) -> Result<Vec<HighLevelValue>, eyre::Error> {
        let mut params = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            let tdef = param.tref.resolve(spec);

            match &tdef.state {
                | None => {
                    params.push(HighLevelValue::Concrete(
                        tdef.wasi.arbitrary_value(spec, self.u)?,
                    ));
                },
                | Some(_state_type) => {
                    let resources = env
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

                    params.push(HighLevelValue::Resource(resource_id));
                },
            }
        }

        Ok(params)
    }

    fn handle_results(
        &mut self,
        _spec: &Spec,
        _function: &Function,
        _env: &mut Environment,
        _params: Vec<(WasiValue, Option<ResourceIdx>)>,
        _results: Vec<Option<ResourceIdx>>,
    ) -> Result<(), eyre::Error> {
        Ok(())
    }
}
