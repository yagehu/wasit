pub mod call;

use std::io;

use color_eyre::eyre;
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use witx::Layout;

use self::call::CallResult;
use crate::{resource_ctx::ResourceContext, seed, store::ExecutionStore};

fn pb_func(name: &str) -> executor_pb::WasiFunc {
    use executor_pb::WasiFunc::*;

    match name {
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
        | "fd_renumber" => FD_RENUMBER,
        | "fd_seek" => FD_SEEK,
        | "fd_sync" => FD_SYNC,
        | "fd_tell" => FD_TELL,
        | "fd_write" => FD_WRITE,
        | "path_create_directory" => PATH_CREATE_DIRECTORY,
        | "path_filestat_get" => PATH_FILESTAT_GET,
        | "path_filestat_set_times" => PATH_FILESTAT_SET_TIMES,
        | "path_link" => PATH_LINK,
        | "path_open" => PATH_OPEN,
        | "path_remove_directory" => PATH_REMOVE_DIRECTORY,
        | "path_rename" => PATH_RENAME,
        | "path_symlink" => PATH_SYMLINK,
        | "path_unlink_file" => PATH_UNLINK_FILE,
        | _ => panic!("{name}"),
    }
}

#[derive(Debug)]
pub struct Prog {
    store:        ExecutionStore,
    executor:     RunningExecutor,
    resource_ctx: ResourceContext,
}

impl Prog {
    pub fn new(executor: RunningExecutor, store: ExecutionStore) -> Result<Self, io::Error> {
        Ok(Self {
            store,
            executor,
            resource_ctx: ResourceContext::new(),
        })
    }

    pub fn call(
        &mut self,
        func: &witx::InterfaceFunc,
        params: Vec<Value>,
        results: Vec<Value>,
    ) -> Result<(), eyre::Error> {
        self.store.recorder_mut().begin_call()?;

        let result_trefs = func.unpack_expected_result();
        let response = self.executor.call(executor_pb::request::Call {
            func:           protobuf::EnumOrUnknown::new(pb_func(func.name.as_str())),
            params:         func
                .params
                .iter()
                .zip(params)
                .map(|(param, v)| v.into_pb_value(param.tref.type_().as_ref()))
                .collect(),
            results:        result_trefs
                .iter()
                .zip(results)
                .map(|(result_tref, v)| v.into_pb_value(result_tref.type_().as_ref()))
                .collect(),
            special_fields: Default::default(),
        })?;
        let errno = match response.errno_option.unwrap() {
            | executor_pb::response::call::Errno_option::ErrnoSome(i) => Some(i),
            | executor_pb::response::call::Errno_option::ErrnoNone(_) => None,
            | _ => panic!(),
        };

        std::thread::sleep(std::time::Duration::from_secs(1));

        self.store.recorder_mut().end_call(CallResult {
            func: func.name.as_str().to_owned(),
            errno,
            params: func
                .params
                .iter()
                .zip(response.params)
                .map(|(param, value)| Value::from_pb_value(value, param.tref.type_().as_ref()))
                .collect(),
            results: result_trefs
                .iter()
                .zip(response.results)
                .map(|(result_tref, result)| {
                    Value::from_pb_value(result, result_tref.type_().as_ref())
                })
                .collect(),
        })?;

        Ok(())
    }

    pub fn resource_ctx(&mut self) -> &ResourceContext {
        &self.resource_ctx
    }

    pub fn resource_ctx_mut(&mut self) -> &mut ResourceContext {
        &mut self.resource_ctx
    }

    pub fn store(&self) -> &ExecutionStore {
        &self.store
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Bitflags(seed::BitflagsValue),
    Variant(VariantValue),
    Handle(u32),
    String(seed::StringValue),
    Pointer(Vec<Value>),
    Builtin(seed::BuiltinValue),
}

impl Value {
    pub fn zero_value_from_spec(tref: &witx::TypeRef) -> Self {
        match tref.type_().as_ref() {
            | witx::Type::Record(_) => todo!(),
            | witx::Type::Variant(_) => todo!(),
            | witx::Type::Handle(_) => Self::Handle(0),
            | witx::Type::List(_) => todo!(),
            | witx::Type::Pointer(_tref) => Self::Pointer(vec![]),
            | witx::Type::ConstPointer(_) => todo!(),
            | witx::Type::Builtin(builtin) => Self::Builtin(match builtin {
                | witx::BuiltinType::Char => todo!(),
                | witx::BuiltinType::U8 { .. } => seed::BuiltinValue::U8(0),
                | witx::BuiltinType::U16 => todo!(),
                | witx::BuiltinType::U32 { .. } => seed::BuiltinValue::U32(0),
                | witx::BuiltinType::U64 => seed::BuiltinValue::U64(0),
                | witx::BuiltinType::S8 => todo!(),
                | witx::BuiltinType::S16 => todo!(),
                | witx::BuiltinType::S32 => todo!(),
                | witx::BuiltinType::S64 => seed::BuiltinValue::S64(0),
                | witx::BuiltinType::F32 => todo!(),
                | witx::BuiltinType::F64 => todo!(),
            }),
        }
    }

    pub fn into_pb_value(self, ty: &witx::Type) -> executor_pb::Value {
        let which = match (ty, self) {
            | (witx::Type::Record(record_type), Value::Bitflags(bitflags))
                if record_type.bitflags_repr().is_some() =>
            {
                executor_pb::value::Which::Bitflags(executor_pb::value::Bitflags {
                    repr:           match record_type.bitflags_repr().unwrap() {
                        | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                        | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                        | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                        | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                    }
                    .into(),
                    members:        bitflags
                        .0
                        .into_iter()
                        .map(|member| executor_pb::value::bitflags::Member {
                            name:           member.name,
                            value:          member.value,
                            special_fields: Default::default(),
                        })
                        .collect(),
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Variant(variant_type), Value::Variant(variant)) => {
                let (case_idx, case) = variant_type
                    .cases
                    .iter()
                    .enumerate()
                    .filter(|(_i, case)| case.name.as_str() == variant.name)
                    .next()
                    .unwrap();
                let payload = match variant.payload {
                    | Some(payload) => {
                        executor_pb::value::variant::Payload_option::PayloadSome(Box::new(
                            payload.into_pb_value(case.tref.as_ref().unwrap().type_().as_ref()),
                        ))
                    },
                    | None => {
                        executor_pb::value::variant::Payload_option::PayloadNone(Default::default())
                    },
                };

                executor_pb::value::Which::Variant(Box::new(executor_pb::value::Variant {
                    case_idx:       case_idx as u64,
                    size:           variant_type.mem_size() as u32,
                    tag_repr:       match variant_type.tag_repr {
                        | witx::IntRepr::U8 => executor_pb::IntRepr::U8,
                        | witx::IntRepr::U16 => executor_pb::IntRepr::U16,
                        | witx::IntRepr::U32 => executor_pb::IntRepr::U32,
                        | witx::IntRepr::U64 => executor_pb::IntRepr::U64,
                    }
                    .into(),
                    payload_offset: variant_type.payload_offset() as u32,
                    payload_option: Some(payload),
                    special_fields: Default::default(),
                }))
            },
            | (_, Value::Handle(handle)) => executor_pb::value::Which::Handle(handle),
            | (_, Value::String(string)) => executor_pb::value::Which::String(string.into()),
            | (witx::Type::Pointer(tref), Value::Pointer(values)) => {
                executor_pb::value::Which::Pointer(executor_pb::value::Array {
                    items:          values
                        .into_iter()
                        .map(|v| v.into_pb_value(tref.type_().as_ref()))
                        .collect(),
                    item_size:      tref.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (_, Value::Builtin(builtin)) => executor_pb::value::Which::Builtin(builtin.into()),
            | (_, Value::Bitflags(_)) => panic!(),
            | (_, Value::Variant(_)) => panic!(),
            | (_, Value::Pointer(_)) => panic!(),
        };

        executor_pb::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    fn from_pb_value(x: executor_pb::Value, ty: &witx::Type) -> Self {
        match (ty, x.which.unwrap()) {
            | (_, executor_pb::value::Which::Builtin(builtin)) => Self::Builtin(builtin.into()),
            | (_, executor_pb::value::Which::String(string)) => {
                Self::String(seed::StringValue::from(string))
            },
            | (_, executor_pb::value::Which::Bitflags(bitflags)) => {
                Self::Bitflags(seed::BitflagsValue(
                    bitflags
                        .members
                        .into_iter()
                        .map(seed::BitflagsMemberValue::from)
                        .collect(),
                ))
            },
            | (_, executor_pb::value::Which::Handle(handle)) => Self::Handle(handle),
            | (_, executor_pb::value::Which::Array(_)) => todo!(),
            | (_, executor_pb::value::Which::Record(_)) => todo!(),
            | (_, executor_pb::value::Which::ConstPointer(_)) => todo!(),
            | (witx::Type::Pointer(tref), executor_pb::value::Which::Pointer(list)) => {
                let mut items = Vec::with_capacity(list.items.len());

                for value in list.items {
                    items.push(Self::from_pb_value(value, tref.type_().as_ref()));
                }

                Self::Pointer(items)
            },
            | (witx::Type::Variant(variant_type), executor_pb::value::Which::Variant(variant)) => {
                let case = &variant_type.cases[variant.case_idx as usize];

                Self::Variant(VariantValue {
                    idx:     variant.case_idx,
                    name:    case.name.as_str().to_owned(),
                    payload: match variant.payload_option.unwrap() {
                        | executor_pb::value::variant::Payload_option::PayloadSome(payload) => {
                            Some(Box::new(Self::from_pb_value(
                                *payload,
                                case.tref.as_ref().unwrap().type_().as_ref(),
                            )))
                        },
                        | executor_pb::value::variant::Payload_option::PayloadNone(_) => None,
                        | _ => todo!(),
                    },
                })
            },
            | (_, executor_pb::value::Which::Variant(_)) => panic!(),
            | (witx::Type::Builtin(_), _)
            | (witx::Type::Record(_), _)
            | (witx::Type::Handle(_), _)
            | (witx::Type::Pointer(_), _)
            | (witx::Type::ConstPointer(_), _)
            | (witx::Type::List(_), _)
            | (witx::Type::Variant(_), _) => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct VariantValue {
    pub idx:     u64,
    pub name:    String,
    pub payload: Option<Box<Value>>,
}

pub(crate) fn register_resource_rec(
    ctx: &mut ResourceContext,
    spec: &witx::Document,
    tref: &witx::TypeRef,
    value: Value,
    resource_id: Option<u64>,
) {
    if let Some(resource) = tref.resource(spec) {
        match resource_id {
            | Some(resource_id) => {
                ctx.register_resource(resource.name.as_str(), value.clone(), resource_id)
            },
            | None => ctx.new_resource(resource.name.as_str(), value.clone()),
        }
    }

    match (tref.type_().as_ref(), value) {
        // | (_, Value::String(_)) => (),
        // | (_, Value::Bitflags(_)) => (),
        // | (witx::Type::Record(record_type), Value::Record(record)) => {
        //     for (member_type, member) in record_type.members.iter().zip(record.0.iter()) {
        //         register_resource_rec(
        //             ctx,
        //             spec,
        //             &member_type.tref,
        //             member
        //                 .to_owned()
        //                 .into_pb_value(member_type.tref.type_().as_ref()),
        //             None,
        //         );
        //     }
        // },
        // | (_, Value::Record(_)) => unreachable!(),
        // | (_, Value::ConstPointer(_)) => todo!(),
        // | (_, Value::List(_)) => todo!(),
        | (witx::Type::Record(record_type), Value::Bitflags(_))
            if record_type.bitflags_repr().is_some() => {},
        | (witx::Type::Variant(variant_type), Value::Variant(variant)) => {
            let case_type = variant_type.cases.get(variant.idx as usize).unwrap();

            if let Some(case_tref) = &case_type.tref {
                register_resource_rec(ctx, spec, case_tref, *variant.payload.unwrap(), None);
            }
        },
        | (_, Value::Handle(_)) => (),
        | (_, Value::String(_)) => (),
        | (_, Value::Pointer(_)) => unimplemented!(),
        | (_, Value::Builtin(_)) => (),
        | (_, Value::Bitflags(_)) => panic!(),
        | (_, Value::Variant(_)) => panic!(),
    }
}

#[cfg(test)]
mod tests {}
