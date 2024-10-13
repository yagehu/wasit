extern crate wazzi_executor_pb_rust as pb;

pub mod normalization;
pub mod spec;

mod resource;
mod strategy;

pub use resource::ResourceIdx;
use resource::{Resource, Resources};
pub use strategy::{CallStrategy, StatefulStrategy, StatelessStrategy};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
};

use eyre::{eyre as err, Context};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spec::{Function, RecordValue, Spec, TypeDef, WasiType, WasiValue};
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_runners::RunningExecutor;
use wazzi_store::TraceStore;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct EnvironmentInitializer {
    preopens: Vec<(String, PathBuf, WasiValue)>,
}

pub fn apply_env_initializers(
    spec: &Spec,
    initializers: &[EnvironmentInitializer],
) -> (Environment, Vec<RuntimeContext>) {
    let mut resources: Resources = Default::default();
    let mut fds: BTreeSet<ResourceIdx> = Default::default();
    let fd_tdef = spec.types.get_by_key("fd").unwrap();
    let fd_type = fd_tdef.state.as_ref().unwrap().record().unwrap();
    let mut preopen_state_members: Vec<WasiValue> = Default::default();
    let mut reverse_resource_index_fd = HashMap::new();
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

    let mut preopens_ids: Option<HashMap<&str, ResourceIdx>> = None;

    for (i, initializer) in initializers.iter().enumerate() {
        let mut preopen_ids_: HashMap<&str, ResourceIdx> = Default::default();

        for (preopen_name, host_path, preopen_value) in &initializer.preopens {
            let resource_id = match &preopens_ids {
                | None => {
                    let state = WasiValue::Record(RecordValue {
                        members: preopen_state_members.clone(),
                    });
                    let resource_idx = resources.push(Resource {
                        state: state.clone(),
                    });

                    reverse_resource_index_fd.insert(state, resource_idx);
                    fds.insert(resource_idx);
                    preopen_ids_.insert(&preopen_name, resource_idx);

                    resource_idx
                },
                | Some(preopens_ids) => *preopens_ids.get(preopen_name.as_str()).unwrap(),
            };

            ctxs[i]
                .resources
                .insert(resource_id, preopen_value.to_owned());
            ctxs[i]
                .preopens
                .insert(resource_id, host_path.to_path_buf());
        }

        if preopens_ids.is_none() {
            preopens_ids = Some(preopen_ids_);
        }
    }

    (
        Environment {
            resources,
            resources_by_types: [("fd".to_string(), fds.clone())].into_iter().collect(),
            resources_types: fds.into_iter().map(|fd| (fd, "fd".to_string())).collect(),
            reverse_resource_index: [("fd".to_string(), reverse_resource_index_fd)]
                .into_iter()
                .collect(),
        },
        ctxs,
    )
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    resources:              Resources,
    resources_by_types:     BTreeMap<String, BTreeSet<ResourceIdx>>,
    resources_types:        HashMap<ResourceIdx, String>,
    reverse_resource_index: HashMap<String, HashMap<WasiValue, ResourceIdx>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            resources:              Default::default(),
            resources_by_types:     Default::default(),
            resources_types:        Default::default(),
            reverse_resource_index: Default::default(),
        }
    }

    pub fn new_resource(&mut self, r#type: String, resource: Resource) -> ResourceIdx {
        let state = resource.state.clone();
        let resource_idx = self.resources.push(resource);

        self.reverse_resource_index
            .entry(r#type.clone())
            .or_default()
            .insert(state, resource_idx);
        self.resources_types.insert(resource_idx, r#type.clone());
        self.resources_by_types
            .entry(r#type)
            .or_default()
            .insert(resource_idx);

        resource_idx
    }

    pub fn call(
        spec: &Spec,
        store: &mut TraceStore<Call>,
        function: &Function,
        strategy: &mut dyn CallStrategy,
        executor: &RunningExecutor,
    ) -> Result<
        (
            Vec<(WasiValue, Option<ResourceIdx>)>,
            Option<Vec<WasiValue>>,
        ),
        eyre::Error,
    > {
        let params = strategy.prepare_arguments(spec, function)?;

        store.begin_call(&Call {
            function: function.name.clone(),
            errno:    None,
            params:   params.clone().into_iter().map(|p| p.0).collect_vec(),
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
                .map(|(param, (value, _idx))| value.into_pb(spec, &param.tref))
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
            params: params.clone().into_iter().map(|p| p.0).collect_vec(),
            results: results.clone(),
        })?;

        Ok((params, results))
    }

    pub fn execute_function_effects(
        &mut self,
        spec: &Spec,
        function: &Function,
        params: &[(WasiValue, Option<ResourceIdx>)],
        results: &Vec<(String, WasiValue)>,
    ) -> Vec<Option<ResourceIdx>> {
        let mut resources: HashMap<&str, ResourceIdx> = Default::default();
        let mut result_resource_idxs = Vec::new();

        for (result, (name, result_value)) in function.results.iter().zip(results.iter()) {
            if let Some(id) = self.register_result_value_resource_recursively(
                spec,
                result.tref.resolve(spec),
                result_value,
            ) {
                result_resource_idxs.push(Some(id));
                resources.insert(name, id);
            } else {
                result_resource_idxs.push(None);
            }
        }

        result_resource_idxs
    }

    pub fn add_resources_to_ctx_recursively(
        &mut self,
        spec: &Spec,
        ctx: &mut RuntimeContext,
        tdef: &TypeDef,
        value: &WasiValue,
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
                    );
                }
            },
            | (WasiType::Record(_), _) => panic!(),
            | (WasiType::Flags(_), _) => (),
            | (WasiType::Pointer(pointer), WasiValue::List(pointer_value)) => {
                for item in &pointer_value.items {
                    self.add_resources_to_ctx_recursively(
                        spec,
                        ctx,
                        pointer.item.resolve(spec),
                        item,
                    );
                }
            },
            | (WasiType::Pointer(_), _) => panic!(),
            | (WasiType::List(list), WasiValue::List(list_value)) => {
                for item in &list_value.items {
                    self.add_resources_to_ctx_recursively(spec, ctx, list.item.resolve(spec), item);
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

            ctx.resources.insert(resource_id, value.to_owned());
        }
    }

    fn register_result_value_resource_recursively(
        &mut self,
        spec: &Spec,
        tdef: &TypeDef,
        value: &WasiValue,
    ) -> Option<ResourceIdx> {
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
            | (WasiType::Pointer(pointer), WasiValue::Pointer(pointer_value)) => {
                for item in &pointer_value.items {
                    self.register_result_value_resource_recursively(
                        spec,
                        pointer.item.resolve(spec),
                        item,
                    );
                }
            },
            | (WasiType::Pointer(_), _) => panic!(),
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Call {
    pub function: String,
    pub errno:    Option<i32>,
    pub params:   Vec<WasiValue>,
    pub results:  Option<Vec<WasiValue>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RuntimeContext {
    pub preopens:  BTreeMap<ResourceIdx, PathBuf>,
    pub resources: BTreeMap<ResourceIdx, WasiValue>,
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self {
            preopens:  Default::default(),
            resources: Default::default(),
        }
    }
}
