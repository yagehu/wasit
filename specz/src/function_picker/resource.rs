use arbitrary::Unstructured;
use wazzi_specz_wasi::{Function, Interface};

use crate::{function_picker::FunctionPicker, resource::Context, Environment};

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
    ) -> Result<&'i Function, eyre::Error> {
        let mut candidates = Vec::new();

        for function in interface.functions.values() {
            let mut is_candidate = true;

            for param in function.params.iter() {
                if param.ty.attributes.is_empty() {
                    continue;
                }

                let resources = match env.resources_by_types.get(param.ty.name.as_ref().unwrap()) {
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
