use std::collections::HashMap;

use color_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;

use crate::{
    call::{CallParam, StringValue},
    capnp_mappers,
    Call,
    Value,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProgSeed {
    mount_base_dir: bool,
    calls:          Vec<Call>,
}

impl ProgSeed {
    #[tracing::instrument]
    pub fn execute(
        &self,
        executor: &mut RunningExecutor,
        spec: &witx::Document,
    ) -> Result<Prog, eyre::Error> {
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();
        let mut resource_ctx = ResourceContext::new();

        if self.mount_base_dir {
            const BASE_DIR_RESOURCE_ID: u64 = 0;

            let base_dir_fd = executor.base_dir_fd();
            let mut message_builder = capnp::message::Builder::new_default();
            let mut decl_builder =
                message_builder.init_root::<wazzi_executor_capnp::decl_request::Builder>();

            decl_builder
                .reborrow()
                .set_resource_id(BASE_DIR_RESOURCE_ID);
            decl_builder.reborrow().init_value().set_handle(base_dir_fd);
            executor
                .decl(decl_builder.into_reader())
                .wrap_err("failed to declare base dir resource")?;
            resource_ctx.insert(
                BASE_DIR_RESOURCE_ID,
                Resource {
                    id: BASE_DIR_RESOURCE_ID,
                },
            );
        }

        for call in &self.calls {
            let mut message_builder = capnp::message::Builder::new_default();
            let mut call_builder =
                message_builder.init_root::<wazzi_executor_capnp::call_request::Builder>();

            match call.func.as_str() {
                | "path_open" => call_builder.set_func(wazzi_executor_capnp::Func::PathOpen),
                | _ => panic!(),
            }

            let func_spec = module_spec
                .func(&witx::Id::new(call.func.as_str()))
                .unwrap();
            let mut params_builder = call_builder
                .reborrow()
                .init_params(func_spec.params.len() as u32);

            for (i, param_spec) in func_spec.params.iter().enumerate() {
                let call_param = call.params.get(i).unwrap();
                let mut param_builder = params_builder.reborrow().get(i as u32);
                let mut type_builder = param_builder.reborrow().init_type();

                capnp_mappers::build_type(param_spec.tref.type_().as_ref(), &mut type_builder);

                match (&param_spec.resource, &call_param) {
                    | (Some(_resource_spec), &&CallParam::Resource(resource_id)) => {
                        let resource = resource_ctx.get(resource_id).unwrap_or_else(|| {
                            panic!("resource {resource_id} not found in the context")
                        });
                        let mut resource_builder = param_builder.reborrow().init_resource();

                        resource_builder.set_id(resource.id);
                    },
                    | (None, &CallParam::Resource(resource_id)) => {
                        panic!(
                            "resource {resource_id} ({}) is not specified as a resource",
                            param_spec.name.as_str()
                        );
                    },
                    | (_, &CallParam::Value(value)) => {
                        let mut value_builder = param_builder.init_value();

                        match (param_spec.tref.type_().as_ref(), value) {
                            | (witx::Type::Pointer(_tref), Value::String(StringValue::Utf8(s))) => {
                                value_builder
                                    .reborrow()
                                    .init_string(s.len() as u32)
                                    .push_str(s);
                            },
                            | (witx::Type::Record(record), Value::Bitflags(bitflags))
                                if record.bitflags_repr().is_some() =>
                            {
                                if bitflags.members.len() != record.members.len() {
                                    panic!(
                                        "bitflags members length mismatch {} vs {}",
                                        record.members.len(),
                                        bitflags.members.len(),
                                    );
                                }

                                let mut bitflags_builder = value_builder.init_bitflags();
                                let mut members_builder = bitflags_builder
                                    .reborrow()
                                    .init_members(record.members.len() as u32);

                                for (i, member) in bitflags.members.iter().enumerate() {
                                    members_builder.set(i as u32, member.value);
                                }
                            },
                            | (
                                witx::Type::List(witx::TypeRef::Value(ty)),
                                Value::String(string),
                            ) if matches!(
                                ty.as_ref(),
                                witx::Type::Builtin(witx::BuiltinType::Char)
                            ) =>
                            {
                                match string {
                                    | StringValue::Utf8(s) => value_builder
                                        .init_string(s.len() as u32)
                                        .reborrow()
                                        .push_str(s),
                                }
                            },
                            | _ => unimplemented!("param_spec is {:#?}", param_spec),
                        }
                    },
                }
            }

            executor
                .call(call_builder.into_reader())
                .wrap_err("failed to call function in executor")?;
        }

        Ok(Prog {
            calls: self.calls.clone(),
            resource_ctx,
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Prog {
    calls:        Vec<Call>,
    resource_ctx: ResourceContext,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Resource {
    id: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct ResourceContext {
    map: HashMap<u64, Resource>,
}

impl ResourceContext {
    fn new() -> Self {
        Self {
            map: Default::default(),
        }
    }

    fn insert(&mut self, id: u64, resource: Resource) {
        self.map.insert(id, resource);
    }

    fn get(&self, id: u64) -> Option<&Resource> {
        self.map.get(&id)
    }
}
