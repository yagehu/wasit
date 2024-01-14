pub use self::error::GrowError;

mod error;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
};

use arbitrary::Unstructured;
use color_eyre::eyre::{self, Context};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;

use crate::{
    call::{
        ArrayValue,
        BuiltinValue,
        Call,
        CallParamSpec,
        CallResultSpec,
        PointerAlloc,
        PointerValue,
        RecordMemberValue,
        RecordValue,
        StringValue,
        Value,
    },
    capnp_mappers,
    snapshot::{store::SnapshotStore, WasiSnapshot},
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProgSeed {
    mount_base_dir: bool,
    calls:          Vec<Call>,
}

impl ProgSeed {
    #[tracing::instrument]
    pub fn execute<S>(
        &self,
        executor: &RunningExecutor,
        spec: &witx::Document,
        snapshot_store: &mut S,
    ) -> Result<Prog, eyre::Error>
    where
        S: SnapshotStore<Snapshot = WasiSnapshot> + fmt::Debug,
        <S as SnapshotStore>::Error: std::error::Error + Send + Sync + 'static,
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
                "fd",
            );
        }

        for call in &self.calls {
            let mut message_builder = capnp::message::Builder::new_default();
            let mut call_builder =
                message_builder.init_root::<wazzi_executor_capnp::call_request::Builder>();

            call_builder.set_func(match call.func.as_str() {
                | "args_get" => wazzi_executor_capnp::Func::ArgsGet,
                | "args_sizes_get" => wazzi_executor_capnp::Func::ArgsSizesGet,
                | "environ_get" => wazzi_executor_capnp::Func::EnvironGet,
                | "environ_sizes_get" => wazzi_executor_capnp::Func::EnvironSizesGet,
                | "clock_res_get" => wazzi_executor_capnp::Func::ClockResGet,
                | "clock_time_get" => wazzi_executor_capnp::Func::ClockTimeGet,
                | "fd_advise" => wazzi_executor_capnp::Func::FdAdvise,
                | "fd_allocate" => wazzi_executor_capnp::Func::FdAllocate,
                | "fd_close" => wazzi_executor_capnp::Func::FdClose,
                | "fd_datasync" => wazzi_executor_capnp::Func::FdDatasync,
                | "fd_fdstat_get" => wazzi_executor_capnp::Func::FdFdstatGet,
                | "fd_read" => wazzi_executor_capnp::Func::FdRead,
                | "fd_seek" => wazzi_executor_capnp::Func::FdSeek,
                | "fd_write" => wazzi_executor_capnp::Func::FdWrite,
                | "path_open" => wazzi_executor_capnp::Func::PathOpen,
                | _ => panic!(),
            });

            let func_spec = module_spec
                .func(&witx::Id::new(call.func.as_str()))
                .unwrap();
            let mut params_builder = call_builder
                .reborrow()
                .init_params(func_spec.params.len() as u32);
            let mut params_resource = vec![None; func_spec.params.len()];

            for (i, param_spec) in func_spec.params.iter().enumerate() {
                let call_param = call.params.get(i).unwrap();
                let mut param_builder = params_builder.reborrow().get(i as u32);
                let mut type_builder = param_builder.reborrow().init_type();

                capnp_mappers::build_type(param_spec.tref.type_().as_ref(), &mut type_builder);

                match (&param_spec.tref.resource(spec), &call_param) {
                    | (&Some(_resource_spec), &&CallParamSpec::Resource(resource_id)) => {
                        let resource = resource_ctx.get(resource_id).unwrap_or_else(|| {
                            panic!("resource {resource_id} not found in the context")
                        });
                        let mut resource_builder = param_builder.reborrow().init_resource();

                        resource_builder.set_id(resource.id);
                        params_resource.get_mut(i).unwrap().replace(resource_id);
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
                for (i, result_tref) in results.iter().enumerate() {
                    match &call.results[i] {
                        | CallResultSpec::Ignore => (),
                        | &CallResultSpec::Resource(resource_id) => {
                            let resource_spec = result_tref.resource(spec).unwrap();

                            resource_ctx.insert(
                                resource_id,
                                Resource { id: resource_id },
                                resource_spec.name.as_str(),
                            );
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

                    // This only applies to fd_close dropping fd.
                    for (i, param_spec) in func_spec.params.iter().enumerate() {
                        if param_spec.drop {
                            resource_ctx.drop(params_resource[i].unwrap());
                        }
                    }

                    Some(0)
                },
                | wazzi_executor_capnp::call_return::Which::Errno(errno) => Some(errno),
            };
            let results_reader = response.get_results()?;

            if errno.is_none() || matches!(errno, Some(0)) {
                for (result_reader, result_tref) in results_reader.iter().zip(results.iter()) {
                    let call_result = capnp_mappers::from_capnp_call_result(
                        result_tref.type_().as_ref(),
                        &result_reader,
                    )?;

                    call_results.push(call_result);
                }
            }

            snapshot_store
                .push_snapshot(WasiSnapshot {
                    errno,
                    params: call.params.clone(),
                    results: call_results,
                    linear_memory: Vec::new(),
                })
                .wrap_err("failed to record snapshot")?;
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

                member_builder
                    .reborrow()
                    .init_name(member_type.name.as_str().len() as u32)
                    .push_str(member_type.name.as_str());

                let mut member_spec_builder = member_builder.reborrow().init_spec();
                let mut member_type_builder = member_spec_builder.reborrow().init_type();

                capnp_mappers::build_type(
                    member_type.tref.type_().as_ref(),
                    &mut member_type_builder,
                );

                match &member_value.value {
                    | &CallParamSpec::Resource(resource_id) => member_spec_builder
                        .reborrow()
                        .init_resource()
                        .set_id(resource_id),
                    | CallParamSpec::Value(value) => {
                        let mut member_value_builder = member_spec_builder.reborrow().init_value();

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
                | &PointerValue::Alloc(PointerAlloc::Resource(resource)) => {
                    alloc_builder.set_resource(resource)
                },
                | &PointerValue::Alloc(PointerAlloc::Value(value)) => {
                    alloc_builder.set_value(value)
                },
            }
        },
        | (witx::Type::Builtin(builtin_type), Value::Builtin(builtin_value)) => {
            let mut builtin_builder = builder.reborrow().init_builtin();

            match (builtin_type, builtin_value) {
                | (witx::BuiltinType::U8 { .. }, &BuiltinValue::U8(i)) => builtin_builder.set_u8(i),
                | (witx::BuiltinType::U32 { .. }, &BuiltinValue::U32(i)) => {
                    builtin_builder.set_u32(i)
                },
                | (witx::BuiltinType::U64, &BuiltinValue::U64(i)) => builtin_builder.set_u64(i),
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
            // variant_builder
            //     .reborrow()
            //     .init_case_name(case.name.as_str().len() as u32)
            //     .push_str(case.name.as_str());

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

impl Prog {
    pub fn grow_by_func(
        &mut self,
        u: &mut Unstructured,
        spec: &witx::Document,
        func_spec: &witx::InterfaceFunc,
    ) -> Result<(), GrowError> {
        let result_trefs = func_spec.unpack_expected_result();
        let mut params = Vec::with_capacity(func_spec.params.len());
        // TODO(huayge):
        let results = Vec::with_capacity(result_trefs.len());

        for param_spec in func_spec.params.iter() {
            params.push(self.pick_or_generate_param(u, spec, &param_spec.tref)?);
        }

        self.calls.push(Call {
            func: func_spec.name.as_str().to_owned(),
            params,
            results,
        });

        Ok(())
    }

    fn pick_or_generate_param(
        &self,
        u: &mut Unstructured,
        spec: &witx::Document,
        tref: &witx::TypeRef,
    ) -> Result<CallParamSpec, GrowError> {
        let mut gen_value = || {
            match tref.type_().as_ref() {
                | witx::Type::Record(record) => {
                    let mut members = Vec::with_capacity(record.members.len());

                    // First, check if the first record member can be fulfilled by the second via alloc.
                    if record.members.len() == 2 {
                        match (
                            record.members[0].tref.resource(spec),
                            record.members[1].tref.resource(spec),
                        ) {
                            | (Some(resource), Some(other_resource))
                                if resource.fulfilled_by(spec).into_iter().any(|candidate| {
                                    if candidate != other_resource {
                                        return false;
                                    }

                                    spec.resource_relation(&candidate.name, &resource.name)
                                        == witx::ResourceRelation::Alloc
                                }) =>
                            {
                                //

                                let member_1 =
                                    self.pick_or_generate_param(u, spec, &record.members[1].tref)?;
                                let member_0 = match &member_1 {
                                    | &CallParamSpec::Resource(resource) => {
                                        CallParamSpec::Value(Value::Pointer(PointerValue::Alloc(
                                            PointerAlloc::Resource(resource),
                                        )))
                                    },
                                    | &CallParamSpec::Value(Value::Builtin(BuiltinValue::U32(
                                        i,
                                    ))) => CallParamSpec::Value(Value::Pointer(
                                        PointerValue::Alloc(PointerAlloc::Value(i)),
                                    )),
                                    | _ => unreachable!(),
                                };

                                return Ok(CallParamSpec::Value(Value::Record(RecordValue(vec![
                                    RecordMemberValue {
                                        name:  record.members[0].name.as_str().to_owned(),
                                        value: member_0,
                                    },
                                    RecordMemberValue {
                                        name:  record.members[1].name.as_str().to_owned(),
                                        value: member_1,
                                    },
                                ]))));
                            },
                            | _ => (),
                        }
                    }

                    for member_spec in &record.members {
                        let member = self.pick_or_generate_param(u, spec, &member_spec.tref)?;

                        members.push(RecordMemberValue {
                            name:  member_spec.name.as_str().to_owned(),
                            value: member,
                        });
                    }

                    Ok(CallParamSpec::Value(Value::Record(RecordValue(members))))
                },
                | witx::Type::Variant(_) => todo!(),
                | witx::Type::Handle(_) => todo!(),
                | witx::Type::List(element_tref) => {
                    let len = u.int_in_range(0..=2)? as usize;
                    let mut elements = Vec::with_capacity(len);
                    let element = self.pick_or_generate_param(u, spec, element_tref)?;

                    elements.push(element);

                    Ok(CallParamSpec::Value(Value::Array(ArrayValue(elements))))
                },
                | witx::Type::Pointer(_) => todo!("{:#?}", tref),
                | witx::Type::ConstPointer(_) => todo!(),
                | witx::Type::Builtin(builtin) => {
                    Ok(CallParamSpec::Value(Value::Builtin(match builtin {
                        | witx::BuiltinType::Char => todo!(),
                        | witx::BuiltinType::U8 { .. } => todo!(),
                        | witx::BuiltinType::U16 => todo!(),
                        | witx::BuiltinType::U32 { .. } => BuiltinValue::U32(u.arbitrary()?),
                        | witx::BuiltinType::U64 => todo!(),
                        | witx::BuiltinType::S8 => todo!(),
                        | witx::BuiltinType::S16 => todo!(),
                        | witx::BuiltinType::S32 => todo!(),
                        | witx::BuiltinType::S64 => todo!(),
                        | witx::BuiltinType::F32 => todo!(),
                        | witx::BuiltinType::F64 => todo!(),
                    })))
                },
            }
        };

        match tref.resource(spec) {
            | Some(resource) => {
                let resource_pools = self
                    .resource_ctx
                    .fulfilling_resource_pools(spec, resource.name.as_str());

                if resource_pools.is_empty() {
                    if let witx::Type::Builtin(_) = tref.type_().as_ref() {
                        return gen_value();
                    }

                    return Err(GrowError::NoResource {
                        name: resource.name.as_str().to_owned(),
                    });
                }

                let resource_pool = *u.choose(&resource_pools)?;
                let resource_pool = resource_pool.iter().collect::<Vec<_>>();
                let resource = **u.choose(&resource_pool)?;

                Ok(CallParamSpec::Resource(resource))
            },
            | None => gen_value(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Resource {
    id: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct ResourceContext {
    map:   HashMap<u64, Resource>,
    types: BTreeMap<String, BTreeSet<u64>>,
}

impl ResourceContext {
    fn new() -> Self {
        Self {
            map:   Default::default(),
            types: Default::default(),
        }
    }

    fn insert(&mut self, id: u64, resource: Resource, r#type: &str) {
        self.map.insert(id, resource);
        self.types.entry(r#type.to_owned()).or_default().insert(id);
    }

    fn drop(&mut self, id: u64) {
        for pool in self.types.values_mut() {
            pool.remove(&id);
        }

        self.map.remove(&id);
    }

    fn get(&self, id: u64) -> Option<&Resource> {
        self.map.get(&id)
    }

    fn fulfilling_resource_pools<'a>(
        &'a self,
        spec: &witx::Document,
        r#type: &str,
    ) -> Vec<&'a BTreeSet<u64>> {
        let resource_spec = spec.resource(&witx::Id::new(r#type)).unwrap();
        let candidate_resource_specs = resource_spec.fulfilled_by(spec);
        let mut resource_pools = Vec::new();

        for candidate in candidate_resource_specs {
            match self.types.get(candidate.name.as_str()) {
                | None => continue,
                | Some(resources) if resources.is_empty() => continue,
                | Some(resources) => resource_pools.push(resources),
            }
        }

        resource_pools
    }
}
