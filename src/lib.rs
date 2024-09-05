pub mod spec;

mod strategy;

pub use strategy::{CallStrategy, StatelessStrategy};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use eyre::eyre as err;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spec::{Function, Spec, TypeDef, WasiType, WasiValue};
use wazzi_executor::RunningExecutor;
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_store::TraceStore;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    resources:          Vec<Resource>,
    resources_by_types: HashMap<String, BTreeSet<usize>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            resources:          Default::default(),
            resources_by_types: Default::default(),
        }
    }

    pub fn next_resource_id(&self) -> usize {
        self.resources.len()
    }

    pub fn new_resource(&mut self, r#type: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(r#type)
            .or_default()
            .insert(self.resources.len() - 1);

        self.resources.len() - 1
    }

    pub fn call(
        &self,
        spec: &Spec,
        store: &mut TraceStore<Call>,
        function: &Function,
        strategy: &mut dyn CallStrategy,
        executor: &RunningExecutor,
    ) -> Result<Option<Vec<WasiValue>>, eyre::Error> {
        let params = strategy.prepare_arguments(spec, function)?;

        store.begin_call(&Call {
            function: function.name.clone(),
            errno:    None,
            params:   params.clone(),
            results:  None,
        })?;

        let response = executor.call(wazzi_executor_pb_rust::request::Call {
            func:           WasiFunc::try_from(function.name.as_str())
                .map_err(|_| err!("unknown WASI function name"))?
                .into(),
            params:         function
                .params
                .iter()
                .zip(params.clone())
                .map(|(param, value)| value.into_pb(spec, &param.tref))
                .collect(),
            results:        function
                .results
                .iter()
                .map(|result| {
                    result
                        .tref
                        .resolve(spec)
                        .wasi
                        .zero_value(spec)
                        .into_pb(spec, &result.tref)
                })
                .collect(),
            special_fields: Default::default(),
        })?;
        let errno = match response.errno_option {
            | Some(wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i)) => Some(i),
            | _ => None,
        };
        let results = match errno {
            | Some(i) if i != 0 => None,
            | _ => Some(
                response
                    .results
                    .into_iter()
                    .zip(function.results.iter())
                    .map(|(result_value, result)| {
                        WasiValue::from_pb(result_value, spec, result.tref.resolve(spec))
                    })
                    .collect_vec(),
            ),
        };

        store.end_call(&Call {
            function: function.name.clone(),
            errno,
            params: params.clone(),
            results: results.clone(),
        })?;

        Ok(results)
    }

    pub fn execute_function_effects(
        &mut self,
        spec: &Spec,
        function: &Function,
        results: &[WasiValue],
    ) {
        for (result, result_value) in function.results.iter().zip(results) {
            self.register_result_value_resource_recursively(
                spec,
                result.tref.resolve(spec),
                result_value,
            );
        }

        todo!()
    }

    fn register_result_value_resource_recursively(
        &mut self,
        spec: &Spec,
        tdef: &TypeDef,
        value: &WasiValue,
    ) {
        // First, register structural members.
        match (&tdef.wasi, value) {
            | (WasiType::Handle, _)
            | (WasiType::S64, _)
            | (WasiType::U8, _)
            | (WasiType::U16, _)
            | (WasiType::U32, _)
            | (WasiType::U64, _) => (),
            | (WasiType::Record(record), WasiValue::Record(record_value)) => {
                for (member, member_value) in record.members.iter().zip(record_value.members.iter())
                {
                    self.register_result_value_resource_recursively(
                        spec,
                        member.tref.resolve(spec),
                        member_value,
                    );
                }
            },
            | (WasiType::Record(_), _) => panic!(),
            | (WasiType::Flags(_), _) => (),
            | (WasiType::List(list), WasiValue::List(list_value)) => {
                for item in &list_value.items {
                    self.register_result_value_resource_recursively(
                        spec,
                        list.item.resolve(spec),
                        item,
                    );
                }
            },
            | (WasiType::List(_), _) => panic!(),
            | (WasiType::String, _) => (),
            | (WasiType::Variant(variant), WasiValue::Variant(variant_value)) => {
                let case = &variant.cases[variant_value.case_idx];

                if let (Some(payload), Some(payload_value)) =
                    (&case.payload, &variant_value.payload)
                {
                    self.register_result_value_resource_recursively(
                        spec,
                        payload.resolve(spec),
                        payload_value,
                    );
                }
            },
            | (WasiType::Variant(_), _) => panic!(),
        }

        if let Some(state) = &tdef.state {
            self.new_resource(
                tdef.name.clone(),
                Resource {
                    state: state.zero_value(spec),
                },
            );
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub state: WasiValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Call {
    pub function: String,
    pub errno:    Option<i32>,
    pub params:   Vec<WasiValue>,
    pub results:  Option<Vec<WasiValue>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RuntimeContext {
    pub resources: BTreeMap<usize, WasiValue>,
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self {
            resources: Default::default(),
        }
    }
}
