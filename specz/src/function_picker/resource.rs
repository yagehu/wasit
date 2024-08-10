use arbitrary::Unstructured;

use crate::{
    function_picker::FunctionPicker,
    preview1::spec::{Function, Interface, Spec, TypeRef},
    resource::Context,
    Environment,
};

/// A function is a candidate if each parameter can be either safely randomly
/// generated, or chosen from a pool of resources.
#[derive(Clone, Copy, Debug)]
pub struct ResourcePicker;

impl FunctionPicker for ResourcePicker {
    fn pick_function<'i>(
        &self,
        u: &mut Unstructured,
        interface: &'i Interface,
        env: &Environment,
        _ctx: &Context,
        spec: &Spec,
    ) -> Result<&'i Function, eyre::Error> {
        let mut candidates = Vec::new();

        for function in interface.functions.values() {
            let mut is_candidate = true;

            for param in function.params.iter() {
                if param.tref.resource_type_def(spec).is_none() {
                    continue;
                }

                let resource_type = match &param.tref {
                    | TypeRef::Named(name) => {
                        let tdef = spec.get_type_def(name).unwrap();

                        match &tdef.attributes {
                            | Some(_attributes) => name,
                            | None => continue,
                        }
                    },
                    | TypeRef::Anonymous(_) => continue,
                };

                let resources = match env.resources_by_types.get(resource_type) {
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
                candidates.push(function);
            }
        }

        Ok(*u.choose(&candidates)?)
    }
}
