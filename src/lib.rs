extern crate wazzi_executor_pb_rust as pb;

pub mod normalization;
pub mod spec;

mod strategy;

pub use strategy::{CallStrategy, StatelessStrategy};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use eyre::eyre as err;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spec::{witx::olang, Function, RecordValue, Spec, TypeDef, WasiType, WasiValue};
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_runners::RunningExecutor;
use wazzi_store::TraceStore;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct EnvironmentInitializer {
    preopens: Vec<(String, WasiValue)>,
}

pub fn apply_env_initializers(
    spec: &Spec,
    initializers: &[EnvironmentInitializer],
) -> (Environment, Vec<RuntimeContext>) {
    let mut resources: Vec<Resource> = Default::default();
    let mut fds: BTreeSet<usize> = Default::default();
    let fd_tdef = spec.types.get_by_key("fd").unwrap();
    let fd_type = fd_tdef.state.as_ref().unwrap().record().unwrap();
    let mut preopen_state_members: Vec<WasiValue> = Default::default();
    let mut ctxs: Vec<RuntimeContext> = vec![RuntimeContext::new(); initializers.len()];

    for member in &fd_type.members {
        preopen_state_members.push(match member.name.as_str() {
            | "offset" => WasiValue::U64(0),
            | "flags" => member
                .tref
                .resolve(spec)
                .wasi
                .flags()
                .unwrap()
                .value([].into_iter().collect()),
            | "type" => member
                .tref
                .resolve(spec)
                .wasi
                .variant()
                .unwrap()
                .value_from_name("directory", None)
                .unwrap(),
            | _ => member.tref.resolve(spec).wasi.zero_value(spec),
        });
    }

    let mut preopens_ids: Option<HashMap<&str, usize>> = None;

    for (i, initializer) in initializers.iter().enumerate() {
        let mut preopen_ids_: HashMap<&str, usize> = Default::default();

        for (preopen_name, preopen_value) in &initializer.preopens {
            let resource_id = match &preopens_ids {
                | None => {
                    resources.push(Resource {
                        state: WasiValue::Record(RecordValue {
                            members: preopen_state_members.clone(),
                        }),
                    });
                    fds.insert(resources.len() - 1);
                    preopen_ids_.insert(&preopen_name, resources.len() - 1);
                    resources.len() - 1
                },
                | Some(preopens_ids) => *preopens_ids.get(preopen_name.as_str()).unwrap(),
            };

            ctxs[i]
                .resources
                .insert(resource_id, preopen_value.to_owned());
        }

        if preopens_ids.is_none() {
            preopens_ids = Some(preopen_ids_);
        }
    }

    (
        Environment {
            resources,
            resources_by_types: [("fd".to_string(), fds)].into_iter().collect(),
        },
        ctxs,
    )
}

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
    ) -> Result<(Vec<WasiValue>, Option<Vec<WasiValue>>), eyre::Error> {
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

        Ok((params, results))
    }

    pub fn execute_function_effects(
        &mut self,
        spec: &Spec,
        function: &Function,
        params: &[WasiValue],
        results: Vec<(String, WasiValue)>,
    ) {
        let mut resources: HashMap<&str, usize> = Default::default();

        for (result, (name, result_value)) in function.results.iter().zip(results.iter()) {
            if let Some(id) = self.register_result_value_resource_recursively(
                spec,
                result.tref.resolve(spec),
                result_value,
            ) {
                resources.insert(name, id);
            }
        }

        for stmt in &function.effects.stmts {
            match stmt {
                | olang::Stmt::RecordFieldSet(record_field_set) => {
                    let value = match &record_field_set.value {
                        | olang::Expr::Param(param_name) => {
                            let (i, _param) = function
                                .params
                                .iter()
                                .enumerate()
                                .find(|(_i, param)| &param.name == param_name)
                                .unwrap();

                            params[i].clone()
                        },
                        | olang::Expr::WasiValue(value) => value.clone(),
                    };
                    let result = function
                        .results
                        .iter()
                        .find(|result| result.name == record_field_set.result)
                        .unwrap();
                    let tdef = result.tref.resolve(spec);
                    let id = *resources.get(record_field_set.result.as_str()).unwrap();
                    let resource = self.resources.get_mut(id).unwrap();
                    let record_type = tdef.state.as_ref().unwrap().record().unwrap();
                    let (i, _field_type) = record_type
                        .members
                        .iter()
                        .enumerate()
                        .find(|(_i, member)| member.name == record_field_set.field)
                        .unwrap();
                    let record = resource.state.record_mut().unwrap();

                    *record.members.get_mut(i).unwrap() = value;
                },
            }
        }
    }

    pub fn add_resources_to_ctx_recursively(
        &self,
        spec: &Spec,
        ctx: &mut RuntimeContext,
        tdef: &TypeDef,
        value: &WasiValue,
        next_idx: &mut usize,
    ) {
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
                    self.add_resources_to_ctx_recursively(
                        spec,
                        ctx,
                        member.tref.resolve(spec),
                        member_value,
                        next_idx,
                    );
                }
            },
            | (WasiType::Record(_), _) => panic!(),
            | (WasiType::Flags(_), _) => (),
            | (WasiType::List(list), WasiValue::List(list_value)) => {
                for item in &list_value.items {
                    self.add_resources_to_ctx_recursively(
                        spec,
                        ctx,
                        list.item.resolve(spec),
                        item,
                        next_idx,
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
                    self.add_resources_to_ctx_recursively(
                        spec,
                        ctx,
                        payload.resolve(spec),
                        payload_value,
                        next_idx,
                    );
                }
            },
            | (WasiType::Variant(_), _) => panic!(),
        }

        if let Some(_state) = &tdef.state {
            ctx.resources.insert(*next_idx, value.to_owned());
            *next_idx += 1;
        }
    }

    fn register_result_value_resource_recursively(
        &mut self,
        spec: &Spec,
        tdef: &TypeDef,
        value: &WasiValue,
    ) -> Option<usize> {
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
            let resource_id = self.new_resource(
                tdef.name.clone(),
                Resource {
                    state: state.zero_value(spec),
                },
            );

            Some(resource_id)
        } else {
            None
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
