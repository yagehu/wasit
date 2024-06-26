use arbitrary::Unstructured;
use wazzi_specz_wasi::Function;

use crate::{param_generator::ParamsGenerator, resource::Context, Environment, Value};

#[derive(Clone, Copy, Debug)]
pub struct StatelessParamsGenerator;

impl ParamsGenerator for StatelessParamsGenerator {
    fn generate_params(
        &self,
        u: &mut Unstructured,
        env: &Environment,
        ctx: &Context,
        function: &Function,
    ) -> Result<Vec<Value>, eyre::Error> {
        let mut params = Vec::with_capacity(function.params.len());
        let mut string_prefix: Option<Vec<u8>> = None;

        for param in function.params.iter() {
            if param.ty.attributes.is_empty() {
                params.push(Value {
                    wasi:     param
                        .ty
                        .wasi
                        .arbitrary_value(u, string_prefix.as_ref().map(|sp| sp.as_slice()))?,
                    resource: None,
                });

                continue;
            }

            let resources = env
                .resources_by_types
                .get(param.ty.name.as_ref().unwrap())
                .unwrap();
            let resource_pool = resources.iter().cloned().collect::<Vec<_>>();
            let resource_idx = *u.choose(&resource_pool)?;
            let (resource, maybe_string_prefix) = ctx.resources.get(&resource_idx).unwrap();

            if let Some(s) = maybe_string_prefix {
                string_prefix = Some(s.clone());
            }

            params.push(Value {
                wasi:     resource.to_owned(),
                resource: Some(resource_idx),
            });
        }

        Ok(params)
    }
}
