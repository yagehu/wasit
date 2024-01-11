use std::collections::HashMap;

use color_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;

use crate::{
    call::{BuiltinValue, CallParamSpec, CallResultSpec, PointerValue, StringValue},
    capnp_mappers,
    Call,
    Recorder,
    SnapshotHandler,
    Value,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProgSeed {
    mount_base_dir: bool,
    calls:          Vec<Call>,
}

impl ProgSeed {
    #[tracing::instrument(skip(recorder))]
    pub fn execute<SH>(
        &self,
        executor: &mut RunningExecutor,
        spec: &witx::Document,
        recorder: &mut Recorder<SH>,
    ) -> Result<Prog, eyre::Error>
    where
        SH: SnapshotHandler,
    {
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
                | "args_get" => call_builder.set_func(wazzi_executor_capnp::Func::ArgsGet),
                | "args_sizes_get" => {
                    call_builder.set_func(wazzi_executor_capnp::Func::ArgsSizesGet)
                },
                | "environ_get" => call_builder.set_func(wazzi_executor_capnp::Func::EnvironGet),
                | "environ_sizes_get" => {
                    call_builder.set_func(wazzi_executor_capnp::Func::EnvironSizesGet)
                },
                | "clock_res_get" => call_builder.set_func(wazzi_executor_capnp::Func::ClockResGet),
                | "clock_time_get" => {
                    call_builder.set_func(wazzi_executor_capnp::Func::ClockTimeGet)
                },
                | "fd_read" => call_builder.set_func(wazzi_executor_capnp::Func::FdRead),
                | "fd_seek" => call_builder.set_func(wazzi_executor_capnp::Func::FdSeek),
                | "fd_write" => call_builder.set_func(wazzi_executor_capnp::Func::FdWrite),
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

                match (&param_spec.tref.resource(spec), &call_param) {
                    | (Some(_resource_spec), &&CallParamSpec::Resource(resource_id)) => {
                        let resource = resource_ctx.get(resource_id).unwrap_or_else(|| {
                            panic!("resource {resource_id} not found in the context")
                        });
                        let mut resource_builder = param_builder.reborrow().init_resource();

                        resource_builder.set_id(resource.id);
                    },
                    | (None, &CallParamSpec::Resource(resource_id)) => {
                        panic!(
                            "resource {resource_id} ({}) is not specified as a resource",
                            param_spec.name.as_str()
                        );
                    },
                    | (_, &CallParamSpec::Value(value)) => {
                        let mut value_builder = param_builder.init_value();

                        build_value(&mut value_builder, param_spec.tref.type_().as_ref(), value);
                    },
                }
            }

            let results = func_spec.unpack_expected_result();
            let mut results_builder = call_builder.reborrow().init_results(results.len() as u32);

            for (i, result_tref) in results.iter().enumerate() {
                let result = &call.results[i];
                let mut result_builder = results_builder.reborrow().get(i as u32);
                let mut type_builder = result_builder.reborrow().init_type();

                capnp_mappers::build_type(result_tref.type_().as_ref(), &mut type_builder);

                match result {
                    | CallResultSpec::Ignore => result_builder.reborrow().set_ignore(()),
                    | &CallResultSpec::Resource(resource_id) => {
                        result_builder.reborrow().set_resource(resource_id)
                    },
                }
            }

            let response = executor
                .call(call_builder.into_reader())
                .wrap_err("failed to call function in executor")?;
            let response = response.get()?;
            let ret = response.get_return()?;
            let mut call_results = Vec::with_capacity(results.len());
            let mut handle_results_ok = || {
                for (i, _result_tref) in results.iter().enumerate() {
                    match &call.results[i] {
                        | CallResultSpec::Ignore => (),
                        | &CallResultSpec::Resource(resource_id) => {
                            resource_ctx.insert(resource_id, Resource { id: resource_id });
                        },
                    }
                }
            };
            let errno = match ret.which()? {
                | wazzi_executor_capnp::call_return::Which::None(_) => {
                    handle_results_ok();

                    None
                },
                | wazzi_executor_capnp::call_return::Which::Errno(0) => {
                    handle_results_ok();

                    Some(0)
                },
                | wazzi_executor_capnp::call_return::Which::Errno(errno) => Some(errno),
            };
            let results_reader = response.get_results()?;

            if errno.is_none() || matches!(errno, Some(0)) {
                for result in results_reader.iter() {
                    let call_result = capnp_mappers::from_capnp_call_result(&result)?;

                    call_results.push(call_result);
                }
            }

            recorder.take_snapshot(errno, call_results);
        }

        Ok(Prog {
            calls: self.calls.clone(),
            resource_ctx,
        })
    }
}

fn build_value(builder: &mut wazzi_executor_capnp::value::Builder, ty: &witx::Type, value: &Value) {
    match (ty, value) {
        | (witx::Type::Pointer(_tref), Value::String(StringValue::Utf8(s))) => {
            builder.reborrow().init_string(s.len() as u32).push_str(s);
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

            let mut bitflags_builder = builder.reborrow().init_bitflags();
            let mut members_builder = bitflags_builder
                .reborrow()
                .init_members(record.members.len() as u32);

            for (i, member) in bitflags.members.iter().enumerate() {
                members_builder.set(i as u32, member.value);
            }
        },
        | (witx::Type::List(witx::TypeRef::Value(ty)), Value::String(string))
            if matches!(ty.as_ref(), witx::Type::Builtin(witx::BuiltinType::Char)) =>
        {
            match string {
                | StringValue::Utf8(s) => {
                    builder.reborrow().init_string(s.len() as u32).push_str(s)
                },
            }
        },
        | (witx::Type::List(tref), Value::Array(array)) => {
            let mut array_builder = builder.reborrow().init_array();
            let mut items_builder = array_builder.reborrow().init_items(array.0.len() as u32);

            for (i, element) in array.0.iter().enumerate() {
                let mut item_builder = items_builder.reborrow().get(i as u32);
                let mut item_type_builder = item_builder.reborrow().init_type();

                capnp_mappers::build_type(tref.type_().as_ref(), &mut item_type_builder);

                match element {
                    | &CallParamSpec::Resource(resource_id) => {
                        item_builder.reborrow().init_resource().set_id(resource_id);
                    },
                    | CallParamSpec::Value(value) => {
                        let mut item_value_builder = item_builder.reborrow().init_value();

                        build_value(&mut item_value_builder, tref.type_().as_ref(), value);
                    },
                }
            }
        },
        | (witx::Type::Record(record_type), Value::Record(record_value)) => {
            let mut record_builder = builder.reborrow().init_record();
            let mut members_builder = record_builder
                .reborrow()
                .init_members(record_type.members.len() as u32);

            for (i, (member_type, member_value)) in record_type
                .members
                .iter()
                .zip(record_value.0.iter())
                .enumerate()
            {
                let mut member_builder = members_builder.reborrow().get(i as u32);
                let mut member_type_builder = member_builder.reborrow().init_type();

                capnp_mappers::build_type(
                    member_type.tref.type_().as_ref(),
                    &mut member_type_builder,
                );

                match &member_value.value {
                    | &CallParamSpec::Resource(resource_id) => member_builder
                        .reborrow()
                        .init_resource()
                        .set_id(resource_id),
                    | CallParamSpec::Value(value) => {
                        let mut member_value_builder = member_builder.reborrow().init_value();

                        build_value(
                            &mut member_value_builder,
                            member_type.tref.type_().as_ref(),
                            value,
                        );
                    },
                }
            }
        },
        | (witx::Type::ConstPointer(tref), Value::ConstPointer(pointer_value)) => {
            let mut const_pointer_builder = builder
                .reborrow()
                .init_const_pointer(pointer_value.0.len() as u32);

            for (i, element_value) in pointer_value.0.iter().enumerate() {
                let mut element_builder = const_pointer_builder.reborrow().get(i as u32);

                build_value(&mut element_builder, tref.type_().as_ref(), element_value);
            }
        },
        | (witx::Type::Pointer(_tref), Value::Pointer(pointer_value)) => {
            let mut pointer_builder = builder.reborrow().init_pointer();
            let mut alloc_builder = pointer_builder.reborrow().init_alloc();

            match pointer_value {
                | &PointerValue::Alloc { resource } => alloc_builder.set_resource_id(resource),
            }
        },
        | (witx::Type::Builtin(builtin_type), Value::Builtin(builtin_value)) => {
            let mut builtin_builder = builder.reborrow().init_builtin();

            match (builtin_type, builtin_value) {
                | (witx::BuiltinType::U8 { .. }, &BuiltinValue::U8(i)) => builtin_builder.set_u8(i),
                | (witx::BuiltinType::U32 { .. }, &BuiltinValue::U32(i)) => {
                    builtin_builder.set_u32(i)
                },
                | (witx::BuiltinType::S64, &BuiltinValue::S64(i)) => builtin_builder.set_s64(i),
                | _ => unimplemented!("{:#?}", builtin_type),
            }
        },
        | (witx::Type::Variant(variant), Value::Variant(variant_value)) => {
            let mut variant_builder = builder.reborrow().init_variant();
            let (case_idx, case) = variant
                .cases
                .iter()
                .enumerate()
                .find(|(_i, case)| case.name.as_str() == variant_value.name)
                .unwrap();

            variant_builder.reborrow().set_case_idx(case_idx as u32);
            variant_builder
                .reborrow()
                .init_case_name(case.name.as_str().len() as u32)
                .push_str(case.name.as_str());

            let mut case_value_builder = variant_builder.reborrow().init_case_value();

            match (&case.tref, &variant_value.payload) {
                | (None, None) => case_value_builder.reborrow().set_none(()),
                | (Some(tref), Some(payload)) => {
                    let mut builder = case_value_builder.reborrow().init_some();

                    capnp_mappers::build_type(
                        tref.type_().as_ref(),
                        &mut builder.reborrow().init_type(),
                    );

                    match payload.as_ref() {
                        | &CallParamSpec::Resource(resource_id) => {
                            builder.reborrow().init_resource().set_id(resource_id)
                        },
                        | CallParamSpec::Value(value) => build_value(
                            &mut builder.reborrow().init_value(),
                            tref.type_().as_ref(),
                            value,
                        ),
                    }
                },
                | _ => panic!(),
            }
        },
        | _ => unimplemented!("type is {:#?} {:#?}", ty, value),
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
