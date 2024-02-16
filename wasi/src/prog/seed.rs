use color_eyre::{eyre, eyre::WrapErr};
use executor_pb::WasiFunc::*;
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use witx::{BuiltinType, Layout};

use super::stateful;
use crate::{
    resource_ctx::{ResourceContext, ResourceId},
    snapshot::{SnapshotStore, WasiSnapshot},
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Prog {
    pub mount_base_dir: bool,
    pub actions:        Vec<Action>,
}

fn prepare_param(
    resource_ctx: &ResourceContext,
    tref: &witx::TypeRef,
    param: &ParamSpec,
) -> executor_pb::Value {
    match param {
        | &ParamSpec::Resource(resource_id) => resource_ctx
            .get_resource(resource_id)
            .unwrap()
            .into_pb_value(tref.type_().as_ref()),
        | ParamSpec::Value(value) => value
            .to_owned()
            .into_pb_value(resource_ctx, tref.type_().as_ref()),
    }
}

fn prepare_result(tref: &witx::TypeRef) -> executor_pb::Value {
    let which = match tref.type_().as_ref() {
        | witx::Type::Record(record) if record.bitflags_repr().is_some() => {
            let mut members = Vec::with_capacity(record.members.len());

            for member in &record.members {
                members.push(executor_pb::value::bitflags::Member {
                    name:           member.name.as_str().to_owned(),
                    value:          false,
                    special_fields: Default::default(),
                });
            }

            executor_pb::value::Which::Bitflags(executor_pb::value::Bitflags {
                repr: protobuf::EnumOrUnknown::new(match record.bitflags_repr().unwrap() {
                    | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                    | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                    | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                    | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                }),
                members,
                special_fields: Default::default(),
            })
        },
        | witx::Type::Record(record) => {
            let mut members = Vec::with_capacity(record.members.len());

            for (member_layout, member) in record
                .member_layout()
                .into_iter()
                .zip(record.members.iter())
            {
                members.push(executor_pb::value::record::Member {
                    name:           member.name.as_str().to_owned(),
                    value:          Some(prepare_result(&member.tref)).into(),
                    offset:         member_layout.offset as u32,
                    special_fields: Default::default(),
                });
            }

            executor_pb::value::Which::Record(executor_pb::value::Record {
                members,
                size: record.mem_size() as u32,
                special_fields: Default::default(),
            })
        },
        | witx::Type::Variant(variant) => {
            executor_pb::value::Which::Variant(Box::new(executor_pb::value::Variant {
                case_idx:       0,
                size:           variant.mem_size() as u32,
                tag_repr:       protobuf::EnumOrUnknown::new(match variant.tag_repr {
                    | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                    | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                    | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                    | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                }),
                payload_offset: 0,
                payload_option: Some(match variant.cases.first().unwrap().tref.as_ref() {
                    | Some(tref) => executor_pb::value::variant::Payload_option::PayloadSome(
                        Box::new(prepare_result(tref)),
                    ),
                    | None => {
                        executor_pb::value::variant::Payload_option::PayloadNone(Default::default())
                    },
                }),
                special_fields: Default::default(),
            }))
        },
        | witx::Type::Handle(_) => executor_pb::value::Which::Handle(0),
        | witx::Type::List(_tref) => unreachable!(),
        | witx::Type::Pointer(_) => unreachable!(),
        | witx::Type::ConstPointer(_) => unreachable!(),
        | witx::Type::Builtin(builtin) => {
            let which = match builtin {
                | BuiltinType::Char => todo!(),
                | BuiltinType::U8 { .. } => executor_pb::value::builtin::Which::U8(0),
                | BuiltinType::U16 => todo!(),
                | BuiltinType::U32 { .. } => executor_pb::value::builtin::Which::U32(0),
                | BuiltinType::U64 => executor_pb::value::builtin::Which::U64(0),
                | BuiltinType::S8 => todo!(),
                | BuiltinType::S16 => todo!(),
                | BuiltinType::S32 => todo!(),
                | BuiltinType::S64 => executor_pb::value::builtin::Which::S64(0),
                | BuiltinType::F32 => todo!(),
                | BuiltinType::F64 => todo!(),
            };

            executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                which:          Some(which),
                special_fields: Default::default(),
            })
        },
    };

    executor_pb::Value {
        which:          Some(which),
        special_fields: Default::default(),
    }
}

impl Prog {
    pub fn execute<S>(
        self,
        executor: &RunningExecutor,
        spec: &witx::Document,
        snapshot_store: &mut S,
    ) -> Result<stateful::Prog, eyre::Error>
    where
        S: SnapshotStore<Snapshot = WasiSnapshot>,
    {
        let mut resource_ctx = ResourceContext::new();
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();

        if self.mount_base_dir {
            const BASE_DIR_RESOURCE_TYPE: &str = "dirfd";

            resource_ctx.new_resource(
                BASE_DIR_RESOURCE_TYPE,
                stateful::Value::Handle(executor.base_dir_fd()),
            );
        }

        let mut calls = Vec::new();

        for action in self.actions {
            match action {
                | Action::Decl(_) => {},
                | Action::Call(call) => {
                    let func = match call.func.as_str() {
                        | "args_get" => ARGS_GET,
                        | "args_sizes_get" => ARGS_SIZES_GET,
                        | "environ_get" => ENVIRON_GET,
                        | "environ_sizes_get" => ENVIRON_SIZES_GET,
                        | "clock_res_get" => CLOCK_RES_GET,
                        | "clock_time_get" => CLOCK_TIME_GET,
                        | "fd_advise" => FD_ADVISE,
                        | "fd_allocate" => FD_ALLOCATE,
                        | "fd_close" => FD_CLOSE,
                        | "fd_datasync" => FD_DATASYNC,
                        | "fd_fdstat_get" => FD_FDSTAT_GET,
                        | "fd_fdstat_set_flags" => FD_FDSTAT_SET_FLAGS,
                        | "fd_fdstat_set_rights" => FD_FDSTAT_SET_RIGHTS,
                        | "fd_filestat_get" => FD_FILESTAT_GET,
                        | "fd_filestat_set_size" => FD_FILESTAT_SET_SIZE,
                        | "fd_filestat_set_times" => FD_FILESTAT_SET_TIMES,
                        | "fd_pread" => FD_PREAD,
                        | "fd_prestat_get" => FD_PRESTAT_GET,
                        | "fd_prestat_dir_name" => FD_PRESTAT_DIR_NAME,
                        | "fd_pwrite" => FD_PWRITE,
                        | "fd_read" => FD_READ,
                        | "fd_readdir" => FD_READDIR,
                        | "fd_seek" => FD_SEEK,
                        | "fd_write" => FD_WRITE,
                        | "path_open" => PATH_OPEN,
                        | _ => panic!("{}", call.func.as_str()),
                    };
                    let func_spec = module_spec
                        .func(&witx::Id::new(call.func.as_str()))
                        .unwrap();
                    let result_trefs = func_spec.unpack_expected_result();
                    let mut params = Vec::with_capacity(func_spec.params.len());
                    let mut results = Vec::with_capacity(result_trefs.len());

                    for (param_type, param) in func_spec.params.iter().zip(call.params.iter()) {
                        params.push(prepare_param(&resource_ctx, &param_type.tref, param));
                    }

                    for result_tref in &result_trefs {
                        results.push(prepare_result(result_tref));
                    }

                    let call_response = executor
                        .call(executor_pb::request::Call {
                            func: protobuf::EnumOrUnknown::new(func),
                            params,
                            results,
                            special_fields: Default::default(),
                        })
                        .wrap_err("failed to call executor")?;

                    let errno = match call_response.errno_option.unwrap() {
                        | executor_pb::response::call::Errno_option::ErrnoSome(errno) => {
                            if errno == 0 {
                                for ((result_value, result_tref), result_spec) in call_response
                                    .results
                                    .iter()
                                    .zip(result_trefs.iter())
                                    .zip(call.results.iter())
                                {
                                    let resource = result_tref.resource(spec).unwrap();

                                    resource_ctx.register_resource(
                                        resource.name.as_str(),
                                        stateful::Value::from_pb_value(result_value.to_owned()),
                                        result_spec.resource,
                                    );
                                }
                            }

                            Some(errno)
                        },
                        | executor_pb::response::call::Errno_option::ErrnoNone(_) => None,
                        | _ => unreachable!(),
                    };

                    calls.push(stateful::Call {
                        func: func_spec.name.as_str().to_owned(),
                        errno,
                        params_post: call_response
                            .params
                            .into_iter()
                            .map(stateful::Value::from_pb_value)
                            .collect(),
                        results: call_response
                            .results
                            .into_iter()
                            .map(stateful::Value::from_pb_value)
                            .collect(),
                    });
                },
            }
        }

        Ok(stateful::Prog { calls })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Decl(Decl),
    Call(Call),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Decl {
    pub resource_id: u64,
    pub value:       SeedValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func: String,

    #[serde(default)]
    pub params: Vec<ParamSpec>,

    #[serde(default)]
    pub results: Vec<ResultSpec>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ParamSpec {
    Resource(u64),
    Value(SeedValue),
}

impl ParamSpec {
    fn into_pb_value(self, resource_ctx: &ResourceContext, ty: &witx::Type) -> executor_pb::Value {
        match self {
            | Self::Value(value) => value.into_pb_value(resource_ctx, ty),
            | Self::Resource(resource_id) => {
                let value = resource_ctx.get_resource(resource_id).unwrap();

                value.into_pb_value(ty)
            },
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ResultSpec {
    pub resource: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SeedValue {
    Builtin(BuiltinValue),
    String(StringValue),
    Bitflags(BitflagsValue),
    Record(RecordValue),
    List(ListValue),
    ConstPointer(ListValue),
    Pointer(PointerValue),
    Variant(VariantValue),
}

impl SeedValue {
    fn into_pb_value(self, resource_ctx: &ResourceContext, ty: &witx::Type) -> executor_pb::Value {
        let which = match (ty, self.clone()) {
            | (_, SeedValue::Builtin(builtin)) => {
                let which = match builtin {
                    | BuiltinValue::U8(i) => executor_pb::value::builtin::Which::U8(i.into()),
                    | BuiltinValue::U32(i) => executor_pb::value::builtin::Which::U32(i),
                    | BuiltinValue::U64(i) => executor_pb::value::builtin::Which::U64(i),
                    | BuiltinValue::S64(i) => executor_pb::value::builtin::Which::S64(i),
                };

                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(which),
                    special_fields: Default::default(),
                })
            },
            | (_, SeedValue::String(string)) => executor_pb::value::Which::String(match string {
                | StringValue::Utf8(s) => s.as_bytes().to_vec(),
                | StringValue::Bytes(b) => b,
            }),
            | (witx::Type::Record(record), SeedValue::Bitflags(bitflags))
                if record.bitflags_repr().is_some() =>
            {
                let mut members = Vec::with_capacity(bitflags.0.len());

                for member in &bitflags.0 {
                    members.push(executor_pb::value::bitflags::Member {
                        name:           member.name.clone(),
                        value:          member.value,
                        special_fields: Default::default(),
                    });
                }

                executor_pb::value::Which::Bitflags(executor_pb::value::Bitflags {
                    repr: protobuf::EnumOrUnknown::new(match record.bitflags_repr().unwrap() {
                        | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                        | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                        | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                        | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                    }),
                    members,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Record(record_type), SeedValue::Record(record)) => {
                let mut members = Vec::with_capacity(record.0.len());

                for ((member_layout, member_type), member) in record_type
                    .member_layout()
                    .into_iter()
                    .zip(record_type.members.iter())
                    .zip(record.0.iter())
                {
                    members.push(executor_pb::value::record::Member {
                        name:           member.name.clone(),
                        value:          Some(
                            member
                                .value
                                .clone()
                                .into_pb_value(resource_ctx, member_type.tref.type_().as_ref()),
                        )
                        .into(),
                        offset:         member_layout.offset as u32,
                        special_fields: Default::default(),
                    });
                }

                executor_pb::value::Which::Record(executor_pb::value::Record {
                    members,
                    size: record_type.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::List(item_tref), SeedValue::List(list)) => {
                let mut items = Vec::with_capacity(list.0.len());

                for item in &list.0 {
                    items.push(
                        item.to_owned()
                            .into_pb_value(resource_ctx, item_tref.type_().as_ref()),
                    );
                }

                executor_pb::value::Which::Array(executor_pb::value::Array {
                    items,
                    item_size: item_tref.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::ConstPointer(item_tref), SeedValue::ConstPointer(list)) => {
                let mut items = Vec::with_capacity(list.0.len());

                for item in &list.0 {
                    items.push(
                        item.to_owned()
                            .into_pb_value(resource_ctx, item_tref.type_().as_ref()),
                    );
                }

                executor_pb::value::Which::ConstPointer(executor_pb::value::Array {
                    items,
                    item_size: item_tref.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Pointer(pointee_tref), SeedValue::Pointer(pointer)) => {
                let resource = resource_ctx
                    .get_resource(pointer.alloc_from_resource)
                    .unwrap();
                let n_items = match resource {
                    | stateful::Value::Builtin(BuiltinValue::U32(i)) => i,
                    | _ => panic!("alloc from resource must be a u32"),
                };

                let mut items = Vec::with_capacity(n_items as usize);

                for _i in 0..n_items {
                    let which = match pointee_tref.type_().as_ref() {
                        | witx::Type::Record(_) => unimplemented!(),
                        | witx::Type::Variant(_) => unimplemented!(),
                        | witx::Type::Handle(_) => unimplemented!(),
                        | witx::Type::List(_) => unimplemented!(),
                        | witx::Type::Pointer(pointee_) => {
                            executor_pb::value::Which::Pointer(executor_pb::value::Array {
                                items:          Vec::with_capacity(0),
                                item_size:      pointee_.mem_size() as u32,
                                special_fields: Default::default(),
                            })
                        },
                        | witx::Type::ConstPointer(pointee_) => {
                            executor_pb::value::Which::ConstPointer(executor_pb::value::Array {
                                items:          Vec::with_capacity(0),
                                item_size:      pointee_.mem_size() as u32,
                                special_fields: Default::default(),
                            })
                        },
                        | witx::Type::Builtin(builtin) => {
                            let which = match builtin {
                                | BuiltinType::Char => unimplemented!(),
                                | BuiltinType::U8 { .. } => {
                                    executor_pb::value::builtin::Which::U8(0)
                                },
                                | BuiltinType::U16 => unimplemented!(),
                                | BuiltinType::U32 { .. } => {
                                    executor_pb::value::builtin::Which::U32(0)
                                },
                                | BuiltinType::U64 => executor_pb::value::builtin::Which::U64(0),
                                | BuiltinType::S8 => unimplemented!(),
                                | BuiltinType::S16 => unimplemented!(),
                                | BuiltinType::S32 => unimplemented!(),
                                | BuiltinType::S64 => executor_pb::value::builtin::Which::S64(0),
                                | BuiltinType::F32 => unimplemented!(),
                                | BuiltinType::F64 => unimplemented!(),
                            };

                            executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                                which:          Some(which),
                                special_fields: Default::default(),
                            })
                        },
                    };
                    let item = executor_pb::Value {
                        which:          Some(which),
                        special_fields: Default::default(),
                    };

                    items.push(item);
                }

                executor_pb::value::Which::Pointer(executor_pb::value::Array {
                    items,
                    item_size: pointee_tref.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Variant(variant_type), SeedValue::Variant(variant)) => {
                let case_idx = variant_type
                    .cases
                    .iter()
                    .enumerate()
                    .filter_map(|(i, case)| {
                        if case.name.as_str() == variant.name {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .next()
                    .unwrap();

                executor_pb::value::Which::Variant(Box::new(executor_pb::value::Variant {
                    case_idx:       case_idx as u64,
                    size:           variant_type.mem_size() as u32,
                    tag_repr:       protobuf::EnumOrUnknown::new(match variant_type.tag_repr {
                        | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                        | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                        | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                        | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                    }),
                    payload_offset: variant_type.payload_offset() as u32,
                    payload_option: Some(executor_pb::value::variant::Payload_option::PayloadNone(
                        Default::default(),
                    )),
                    special_fields: Default::default(),
                }))
            },
            | _ => panic!("spec and value mismatch {:#?}", self),
        };

        executor_pb::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinValue {
    U8(u8),
    U32(u32),
    U64(u64),
    S64(i64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl From<Vec<u8>> for StringValue {
    fn from(x: Vec<u8>) -> Self {
        match String::from_utf8(x) {
            | Ok(s) => Self::Utf8(s),
            | Err(err) => Self::Bytes(err.into_bytes()),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsValue(pub Vec<BitflagsMember>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsMember {
    pub name:  String,
    pub value: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordValue(pub Vec<RecordMember>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordMember {
    pub name:  String,
    pub value: ParamSpec,
}

impl From<BitflagsMember> for executor_pb::value::bitflags::Member {
    fn from(x: BitflagsMember) -> Self {
        Self {
            name:           x.name,
            value:          x.value,
            special_fields: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ListValue(pub Vec<SeedValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PointerValue {
    pub alloc_from_resource: ResourceId,
    pub default_value:       Option<Box<SeedValue>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name: String,
}
