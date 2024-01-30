pub use self::error::GrowError;

mod error;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use arbitrary::Unstructured;
use color_eyre::eyre::{self, Context, ContextCompat};
use executor_pb::WasiFunc::{
    WASI_FUNC_ARGS_GET,
    WASI_FUNC_ARGS_SIZES_GET,
    WASI_FUNC_CLOCK_RES_GET,
    WASI_FUNC_CLOCK_TIME_GET,
    WASI_FUNC_ENVIRON_GET,
    WASI_FUNC_ENVIRON_SIZES_GET,
    WASI_FUNC_FD_ADVISE,
    WASI_FUNC_FD_ALLOCATE,
    WASI_FUNC_FD_CLOSE,
    WASI_FUNC_FD_DATASYNC,
    WASI_FUNC_FD_FDSTAT_GET,
    WASI_FUNC_FD_FDSTAT_SET_FLAGS,
    WASI_FUNC_FD_FDSTAT_SET_RIGHTS,
    WASI_FUNC_FD_FILESTAT_GET,
    WASI_FUNC_FD_READ,
    WASI_FUNC_FD_SEEK,
    WASI_FUNC_FD_WRITE,
    WASI_FUNC_PATH_OPEN,
};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;

use crate::{
    call::{
        ArrayValue,
        BuiltinValue,
        Call,
        CallResultSpec,
        PointerAlloc,
        PointerValue,
        RawValue,
        RecordMemberValue,
        RecordValue,
        StringValue,
        Value,
    },
    pb,
    snapshot::{store::SnapshotStore, ValueView, WasiSnapshot},
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ProgSeed {
    mount_base_dir: bool,
    calls:          Vec<Call>,
}

fn handle_param(
    resource_ctx: &ResourceContext,
    param_tref: &witx::TypeRef,
    param_value: &Value,
) -> Result<executor_pb::ValueSpec, eyre::Error> {
    Ok(match param_value {
        | &Value::Resource(resource_id) => {
            let resource = resource_ctx
                .get(resource_id)
                .wrap_err(format!("resource {resource_id} not in context"))?;

            executor_pb::ValueSpec {
                special_fields: protobuf::SpecialFields::new(),
                type_:          Some(pb::to_type(param_tref.type_().as_ref())).into(),
                which:          Some(executor_pb::value_spec::Which::Resource(
                    executor_pb::Resource {
                        id:             resource.id,
                        special_fields: protobuf::SpecialFields::new(),
                    },
                )),
            }
        },
        | Value::RawValue(value) => {
            let raw_value = match (param_tref.type_().as_ref(), value) {
                | (witx::Type::Builtin(_), RawValue::Builtin(builtin)) => {
                    executor_pb::raw_value::Which::Builtin(executor_pb::raw_value::Builtin {
                        special_fields: protobuf::SpecialFields::new(),
                        which:          Some(match builtin {
                            | &BuiltinValue::U8(i) => {
                                executor_pb::raw_value::builtin::Which::U8(i as u32)
                            },
                            | &BuiltinValue::U32(i) => {
                                executor_pb::raw_value::builtin::Which::U32(i)
                            },
                            | &BuiltinValue::U64(i) => {
                                executor_pb::raw_value::builtin::Which::U64(i)
                            },
                            | &BuiltinValue::S64(i) => {
                                executor_pb::raw_value::builtin::Which::S64(i)
                            },
                        }),
                    })
                },
                | (witx::Type::Pointer(_), RawValue::String(_)) => todo!(),
                | (witx::Type::Record(record), RawValue::Bitflags(bitflags))
                    if record.bitflags_repr().is_some() =>
                {
                    let mut members = Vec::with_capacity(record.members.len());

                    for member in &bitflags.members {
                        members.push(member.value);
                    }

                    executor_pb::raw_value::Which::Bitflags(executor_pb::raw_value::Bitflags {
                        members,
                        special_fields: protobuf::SpecialFields::new(),
                    })
                },
                | (witx::Type::Handle(_), RawValue::Handle(_)) => todo!(),
                | (witx::Type::List(element_tref), RawValue::Array(array)) => {
                    let mut items = Vec::with_capacity(array.0.len());

                    for value in &array.0 {
                        items.push(handle_param(resource_ctx, element_tref, value)?);
                    }

                    executor_pb::raw_value::Which::Array(executor_pb::raw_value::Array {
                        items,
                        special_fields: protobuf::SpecialFields::new(),
                    })
                },
                | (witx::Type::List(_), RawValue::String(string)) => {
                    let bytes = match string {
                        | StringValue::Utf8(s) => s.as_bytes(),
                    };

                    executor_pb::raw_value::Which::String(bytes.to_vec())
                },
                | (witx::Type::Record(record_type), RawValue::Record(record_value)) => {
                    let mut members = Vec::with_capacity(record_type.members.len());

                    for (member_type, record_value) in
                        record_type.members.iter().zip(record_value.0.iter())
                    {
                        members.push(executor_pb::raw_value::record::Member {
                            name:           member_type.name.as_str().to_owned().into_bytes(),
                            value:          Some(handle_param(
                                resource_ctx,
                                &member_type.tref,
                                &record_value.value,
                            )?)
                            .into(),
                            special_fields: protobuf::SpecialFields::new(),
                        });
                    }

                    executor_pb::raw_value::Which::Record(executor_pb::raw_value::Record {
                        members,
                        special_fields: protobuf::SpecialFields::new(),
                    })
                },
                | (witx::Type::ConstPointer(tref), RawValue::ConstPointer(const_pointer)) => {
                    let mut items = Vec::with_capacity(const_pointer.0.len());

                    for item_value in &const_pointer.0 {
                        items.push(handle_param(resource_ctx, tref, item_value)?);
                    }

                    executor_pb::raw_value::Which::ConstPointer(
                        executor_pb::raw_value::ConstPointer {
                            items,
                            special_fields: protobuf::SpecialFields::new(),
                        },
                    )
                },
                | (witx::Type::Pointer(tref), RawValue::Pointer(pointer)) => {
                    let alloc = match pointer {
                        | PointerValue::Alloc(alloc) => {
                            let value_spec = match alloc {
                                | &PointerAlloc::Resource(resource_id) => {
                                    executor_pb::value_spec::Which::Resource(
                                        executor_pb::Resource {
                                            id: resource_id,
                                            special_fields: protobuf::SpecialFields::new(),
                                        },
                                    )
                                },
                                | &PointerAlloc::Value(i) => {
                                    executor_pb::value_spec::Which::RawValue(Box::new(
                                        executor_pb::RawValue {
                                            which: Some(executor_pb::raw_value::Which::Builtin(
                                                executor_pb::raw_value::Builtin {
                                                    which: Some(
                                                        executor_pb::raw_value::builtin::Which::U32(
                                                            i,
                                                        ),
                                                    ),
                                                    special_fields: protobuf::SpecialFields::new(),
                                                },
                                            )),
                                            special_fields: protobuf::SpecialFields::new(),
                                        },
                                    ))
                                },
                            };

                            executor_pb::ValueSpec {
                                type_:          Some(pb::to_type(tref.type_().as_ref())).into(),
                                which:          Some(value_spec),
                                special_fields: protobuf::SpecialFields::new(),
                            }
                        },
                    };

                    executor_pb::raw_value::Which::Pointer(executor_pb::raw_value::Pointer {
                        alloc:          Some(alloc).into(),
                        special_fields: protobuf::SpecialFields::new(),
                    })
                },
                | (witx::Type::Variant(variant_type), RawValue::Variant(variant_value)) => {
                    let case_idx = variant_type
                        .cases
                        .iter()
                        .enumerate()
                        .find_map(|(i, case)| {
                            if case.name.as_str() == variant_value.name {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let payload = match &variant_value.payload {
                        | None => None,
                        | Some(payload) => {
                            Some(executor_pb::raw_value::variant::Optional_payload::Payload(
                                Box::new(handle_param(
                                    resource_ctx,
                                    variant_type.cases[case_idx].tref.as_ref().unwrap(),
                                    payload,
                                )?),
                            ))
                        },
                    };

                    executor_pb::raw_value::Which::Variant(Box::new(
                        executor_pb::raw_value::Variant {
                            case_idx:         case_idx as u32,
                            case_name:        variant_value.name.clone().into_bytes(),
                            optional_payload: payload,
                            special_fields:   protobuf::SpecialFields::new(),
                        },
                    ))
                },
                | x => unreachable!("{:?}", x),
            };

            executor_pb::ValueSpec {
                special_fields: protobuf::SpecialFields::new(),
                type_:          Some(pb::to_type(param_tref.type_().as_ref())).into(),
                which:          Some(executor_pb::value_spec::Which::RawValue(Box::new(
                    executor_pb::RawValue {
                        special_fields: protobuf::SpecialFields::new(),
                        which:          Some(raw_value),
                    },
                ))),
            }
        },
    })
}

fn handle_params(
    resource_ctx: &ResourceContext,
    param_specs: &[witx::InterfaceFuncParam],
    value_specs: &[Value],
) -> Result<Vec<executor_pb::ValueSpec>, eyre::Error> {
    let mut params = Vec::with_capacity(param_specs.len());

    for (param_spec, param_value) in param_specs.iter().zip(value_specs.iter()) {
        params.push(handle_param(resource_ctx, &param_spec.tref, param_value)?);
    }

    Ok(params)
}

fn call_results_ok(
    spec: &witx::Document,
    resource_ctx: &mut ResourceContext,
    result_trefs: &[witx::TypeRef],
    result_specs: &[CallResultSpec],
) {
    for (result_tref, result_spec) in result_trefs.iter().zip(result_specs.iter()) {
        match result_spec {
            | CallResultSpec::Ignore => (),
            | &CallResultSpec::Resource(id) => resource_ctx.insert(
                id,
                Resource { id },
                result_tref.resource(spec).unwrap().name.as_str(),
            ),
        }
    }
}

fn handle_call_param_views(
    _param_specs: &[witx::InterfaceFuncParam],
    call_response: &executor_pb::response::Call,
) -> Vec<ValueView> {
    Vec::with_capacity(call_response.params.len())

    // for (param_spec, param_view) in param_specs.iter().zip(call_response.params.iter()) {
    //     fn handle_one(spec: &witx::Type, param_view: &executor_pb::ValueView) -> ValueView {
    //         let value = match (spec, param_view.content.which.as_ref().unwrap()) {
    //             | (witx::Type::List(item_tref), executor_pb::pure_value::Which::List(list)) => {
    //                 let mut items = Vec::with_capacity(list.items.len());

    //                 for item in &list.items {
    //                     items.push(handle_one(item_tref.type_().as_ref(), item));
    //                 }

    //                 PureValue::List(items)
    //             },
    //             | (witx::Type::Handle(_), &executor_pb::pure_value::Which::Handle(handle)) => {
    //                 PureValue::Handle(handle)
    //             },
    //             | (
    //                 witx::Type::Record(record_type),
    //                 executor_pb::pure_value::Which::Record(record_value),
    //             ) => PureValue::Record(todo!("{:#?}", record_value)),
    //             | _ => unreachable!("{:#?}", param_view),
    //         };

    //         ValueView {
    //             memory_offset: param_view.memory_offset as usize,
    //             memory_len: spec.mem_size(),
    //             value,
    //         }
    //     }

    //     views.push(handle_one(param_spec.tref.type_().as_ref(), param_view));
    // }
}

impl ProgSeed {
    #[tracing::instrument(skip(snapshot_store))]
    pub fn execute<S>(
        &self,
        executor: &RunningExecutor,
        spec: &witx::Document,
        snapshot_store: &mut S,
    ) -> Result<Prog, eyre::Error>
    where
        S: SnapshotStore<Snapshot = WasiSnapshot>,
        <S as SnapshotStore>::Error: std::error::Error + Send + Sync + 'static,
    {
        let mut resource_ctx = ResourceContext::new();
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();

        if self.mount_base_dir {
            const BASE_DIR_RESOURCE_ID: u64 = 0;

            executor
                .decl(executor_pb::request::Decl {
                    resource_id:    BASE_DIR_RESOURCE_ID,
                    value:          Some(executor_pb::RawValue {
                        which:          Some(executor_pb::raw_value::Which::Handle(
                            executor_pb::raw_value::Handle {
                                value:          executor.base_dir_fd(),
                                special_fields: protobuf::SpecialFields::new(),
                            },
                        )),
                        special_fields: protobuf::SpecialFields::new(),
                    })
                    .into(),
                    special_fields: protobuf::SpecialFields::new(),
                })
                .wrap_err("failed to declare base dir resource")?;
            resource_ctx.insert(
                BASE_DIR_RESOURCE_ID,
                Resource {
                    id: BASE_DIR_RESOURCE_ID,
                },
                "fd",
            );
        }

        for call in self.calls.iter() {
            let func = match call.func.as_str() {
                | "args_get" => WASI_FUNC_ARGS_GET,
                | "args_sizes_get" => WASI_FUNC_ARGS_SIZES_GET,
                | "environ_get" => WASI_FUNC_ENVIRON_GET,
                | "environ_sizes_get" => WASI_FUNC_ENVIRON_SIZES_GET,
                | "clock_res_get" => WASI_FUNC_CLOCK_RES_GET,
                | "clock_time_get" => WASI_FUNC_CLOCK_TIME_GET,
                | "fd_advise" => WASI_FUNC_FD_ADVISE,
                | "fd_allocate" => WASI_FUNC_FD_ALLOCATE,
                | "fd_close" => WASI_FUNC_FD_CLOSE,
                | "fd_datasync" => WASI_FUNC_FD_DATASYNC,
                | "fd_fdstat_get" => WASI_FUNC_FD_FDSTAT_GET,
                | "fd_fdstat_set_flags" => WASI_FUNC_FD_FDSTAT_SET_FLAGS,
                | "fd_fdstat_set_rights" => WASI_FUNC_FD_FDSTAT_SET_RIGHTS,
                | "fd_filestat_get" => WASI_FUNC_FD_FILESTAT_GET,
                | "fd_read" => WASI_FUNC_FD_READ,
                | "fd_seek" => WASI_FUNC_FD_SEEK,
                | "fd_write" => WASI_FUNC_FD_WRITE,
                | "path_open" => WASI_FUNC_PATH_OPEN,
                | _ => panic!("{}", call.func.as_str()),
            };
            let func_spec = module_spec
                .func(&witx::Id::new(call.func.as_str()))
                .unwrap();
            let params = handle_params(&resource_ctx, &func_spec.params, &call.params)?;
            let mut results = Vec::with_capacity(call.results.len());
            let result_trefs = func_spec.unpack_expected_result();

            for (result_tref, result_spec) in result_trefs.iter().zip(call.results.iter()) {
                let pb_type = Some(pb::to_type(result_tref.type_().as_ref()));
                let which = match result_spec {
                    | CallResultSpec::Ignore => {
                        executor_pb::result_spec::Which::Ignore(Default::default())
                    },
                    | &CallResultSpec::Resource(resource_id) => {
                        executor_pb::result_spec::Which::Resource(executor_pb::Resource {
                            id:             resource_id,
                            special_fields: protobuf::SpecialFields::new(),
                        })
                    },
                };

                results.push(executor_pb::ResultSpec {
                    type_:          pb_type.into(),
                    which:          Some(which),
                    special_fields: protobuf::SpecialFields::new(),
                });
            }

            let call_request = executor_pb::request::Call {
                func: protobuf::EnumOrUnknown::new(func),
                params,
                results,
                special_fields: protobuf::SpecialFields::new(),
            };
            let call_response = executor.call(call_request)?;
            let errno = match call_response
                .return_
                .as_ref()
                .unwrap()
                .which
                .as_ref()
                .unwrap()
            {
                | executor_pb::return_value::Which::None(_) => {
                    call_results_ok(spec, &mut resource_ctx, &result_trefs, &call.results);

                    None
                },
                | &executor_pb::return_value::Which::Errno(errno) => {
                    if errno == 0 {
                        call_results_ok(spec, &mut resource_ctx, &result_trefs, &call.results);

                        // This only applies to fd_close dropping fd.
                        for (i, param_spec) in func_spec.params.iter().enumerate() {
                            if param_spec.drop {
                                match call.params[i] {
                                    | Value::Resource(resource) => resource_ctx.drop(resource),
                                    | Value::RawValue(_) => todo!(),
                                }
                            }
                        }
                    }

                    Some(errno)
                },
                | _ => unreachable!(),
            };
            let param_views = handle_call_param_views(&func_spec.params, &call_response);

            snapshot_store
                .push_snapshot(WasiSnapshot {
                    errno,
                    params: call.params.clone(),
                    param_views,
                    results: Vec::new(),
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
    ) -> Result<Value, GrowError> {
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
                                    | &Value::Resource(resource) => {
                                        Value::RawValue(RawValue::Pointer(PointerValue::Alloc(
                                            PointerAlloc::Resource(resource),
                                        )))
                                    },
                                    | &Value::RawValue(RawValue::Builtin(BuiltinValue::U32(i))) => {
                                        Value::RawValue(RawValue::Pointer(PointerValue::Alloc(
                                            PointerAlloc::Value(i),
                                        )))
                                    },
                                    | _ => unreachable!(),
                                };

                                return Ok(Value::RawValue(RawValue::Record(RecordValue(vec![
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

                    Ok(Value::RawValue(RawValue::Record(RecordValue(members))))
                },
                | witx::Type::Variant(_) => todo!(),
                | witx::Type::Handle(_) => todo!(),
                | witx::Type::List(element_tref) => {
                    let len = u.int_in_range(0..=2)? as usize;
                    let mut elements = Vec::with_capacity(len);
                    let element = self.pick_or_generate_param(u, spec, element_tref)?;

                    elements.push(element);

                    Ok(Value::RawValue(RawValue::Array(ArrayValue(elements))))
                },
                | witx::Type::Pointer(_) => todo!("{:#?}", tref),
                | witx::Type::ConstPointer(_) => todo!(),
                | witx::Type::Builtin(builtin) => {
                    Ok(Value::RawValue(RawValue::Builtin(match builtin {
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

                Ok(Value::Resource(resource))
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
