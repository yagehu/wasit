pub mod function_picker;
pub mod param_generator;
pub mod preview1;
pub mod resource;
pub mod slang;
pub mod solve;

use std::collections::{BTreeSet, HashMap};

use arbitrary::Unstructured;
use eyre::{eyre as err, Context as _};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_store::TraceStore;

use self::resource::Context;
use crate::{
    param_generator::ParamsGenerator,
    preview1::{
        spec::{Function, Spec, WasiValue},
        witx::elang,
    },
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Call {
    pub function: String,
    pub errno:    Option<i32>,
    pub params:   Vec<Value>,
    pub results:  Vec<Value>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Value {
    pub wasi:     WasiValue,
    pub resource: Option<usize>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub attributes: HashMap<String, WasiValue>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    spec:               Spec,
    resources:          Vec<Resource>,
    resources_by_types: HashMap<String, BTreeSet<usize>>,
}

impl Environment {
    pub fn preview1() -> Result<Self, eyre::Error> {
        let mut spec = Spec::new();

        preview1::witx::preview1(&mut spec)?;

        Ok(Environment {
            spec,
            resources: Default::default(),
            resources_by_types: Default::default(),
        })
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    pub fn new_resource(&mut self, type_name: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(type_name)
            .or_default()
            .insert(self.resources.len() - 1);
        self.resources.len() - 1
    }

    pub fn execute_function_effects(
        &mut self,
        function: &Function,
        result_resources: HashMap<String, usize>,
    ) -> Result<(), eyre::Error> {
        for stmt in function.effects.stmts.iter() {
            match stmt {
                | elang::Stmt::AttrSet(attr_set) => {
                    let new_attr_value = self.eval_effects_expr(&attr_set.value);
                    let resource_id = *result_resources
                        .get(&attr_set.resource)
                        .expect(&attr_set.resource);
                    let resource = self.resources.get_mut(resource_id).unwrap();
                    let attribute = resource.attributes.get_mut(&attr_set.attr).unwrap();

                    *attribute = new_attr_value;
                },
            }
        }

        Ok(())
    }

    fn eval_effects_expr(&self, expr: &elang::Expr) -> WasiValue {
        match expr {
            | elang::Expr::WasiValue(value) => value.clone(),
        }
    }

    pub fn call(
        &self,
        u: &mut Unstructured,
        ctx: &mut Context,
        executor: &RunningExecutor,
        store: &mut TraceStore<Call>,
        function: &Function,
        params_generator: &dyn ParamsGenerator,
    ) -> Result<(bool, Vec<Value>), eyre::Error> {
        let params = params_generator.generate_params(u, self, ctx, function)?;
        let mut next_resource_id = self.resources.len();
        let results = function
            .results
            .iter()
            .map(|result| Value {
                wasi:     result.tref.arbitrary_value(&self.spec, u, None).unwrap(),
                resource: result.tref.resource_type_def(&self.spec).map(|_| {
                    let id = next_resource_id;

                    next_resource_id += 1;
                    id
                }),
            })
            .collect::<Vec<_>>();

        store.begin_call(&Call {
            function: function.name.to_string(),
            errno:    None,
            params:   params.clone(),
            results:  results.clone(),
        })?;

        let response = executor
            .call(wazzi_executor_pb_rust::request::Call {
                func:           WasiFunc::try_from(function.name.as_str())
                    .map_err(|_| err!("unknown wasi function name"))?
                    .into(),
                params:         function
                    .params
                    .iter()
                    .zip(params.clone())
                    .map(|(param, value)| value.wasi.into_pb(&self.spec, &param.tref))
                    .collect(),
                results:        function
                    .results
                    .iter()
                    .zip(results.clone())
                    .map(|(result, value)| value.wasi.into_pb(&self.spec, &result.tref))
                    .collect(),
                special_fields: Default::default(),
            })
            .wrap_err("failed to call")?;
        let results = response
            .results
            .clone()
            .into_iter()
            .zip(results)
            .zip(function.results.iter())
            .map(|((result_value, before), result)| Value {
                wasi:     WasiValue::from_pb(&self.spec, &result.tref, result_value),
                resource: before.resource,
            })
            .collect::<Vec<_>>();
        let ok = match response.errno_option.as_ref().unwrap() {
            | &wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i) => i == 0,
            | wazzi_executor_pb_rust::response::call::Errno_option::ErrnoNone(_) => true,
            | _ => todo!(),
        };

        if ok {
            for result in results.iter() {
                if let Some(id) = result.resource {
                    ctx.resources.insert(id, (result.wasi.clone(), None));
                }
            }
        }

        store.end_call(&Call {
            function: function.name.to_string(),
            // TODO(huyage)
            errno: match response.errno_option.as_ref().unwrap() {
                | &wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i) => Some(i),
                | wazzi_executor_pb_rust::response::call::Errno_option::ErrnoNone(_) => None,
                | _ => todo!(),
            },
            params,
            results: results.clone(),
        })?;

        Ok((ok, results))
    }
}
