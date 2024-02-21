use std::{collections::HashSet, rc::Rc};

use arbitrary::Unstructured;
use color_eyre::eyre::{self, Context};
use wazzi_executor::RunningExecutor;
use witx::{IntRepr, Layout};

use super::{
    pb_func,
    seed::{self, prepare_result},
};
use crate::{prog::r#final, resource_ctx::ResourceContext, FinalProg, SnapshotStore, WasiSnapshot};

#[derive(Debug)]
pub struct Prog<S> {
    pub(crate) store:        S,
    pub(crate) executor:     RunningExecutor,
    pub(crate) resource_ctx: ResourceContext,
    pub(crate) calls:        Vec<Call>,
}

static FUNC_BLACKLIST: once_cell::sync::Lazy<HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        vec![
            "proc_exit",
            "poll_oneoff",
            "proc_raise",
            "sock_accept",
            "sock_recv",
            "sock_send",
            "sock_shutdown",
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    });

impl<S> Prog<S>
where
    S: SnapshotStore<Snapshot = WasiSnapshot>,
{
    pub fn arbitrary_grow(
        &mut self,
        u: &mut Unstructured,
        spec: &witx::Document,
    ) -> Result<(), eyre::Error> {
        let func_pool = self.get_func_pool(spec);
        let func_spec = u.choose(&func_pool)?;

        for (i, param_spec) in func_spec.params.iter().enumerate() {
            match &param_spec.tref {
                | witx::TypeRef::Name(_) => todo!(),
                | witx::TypeRef::Value(ty) => match ty.as_ref() {
                    | witx::Type::Record(_) => unimplemented!(),
                    | witx::Type::Variant(_) => todo!(),
                    | witx::Type::Handle(_) => unimplemented!(),
                    | witx::Type::List(_) => todo!(),
                    | witx::Type::Pointer(_) => todo!(),
                    | witx::Type::ConstPointer(_) => todo!(),
                    | witx::Type::Builtin(_) => todo!(),
                },
            }
        }

        Ok(())
    }

    pub fn grow(
        &mut self,
        spec: &witx::Document,
        func_spec: &witx::InterfaceFunc,
        params: Vec<Value>,
    ) -> Result<(), eyre::Error> {
        let result_trefs = func_spec.unpack_expected_result();
        let call_response = self
            .executor
            .call(executor_pb::request::Call {
                func:           protobuf::EnumOrUnknown::new(pb_func(func_spec.name.as_str())),
                params:         params
                    .into_iter()
                    .zip(func_spec.params.iter())
                    .map(|(p, param_type)| p.into_pb_value(param_type.tref.type_().as_ref()))
                    .collect(),
                results:        result_trefs
                    .iter()
                    .map(|tref| prepare_result(tref))
                    .collect(),
                special_fields: Default::default(),
            })
            .wrap_err("failed to call executor")?;

        let register_results = || {
            for (result_tref, result_value) in result_trefs.iter().zip(call_response.results) {
                self.register_result_resource(
                    spec,
                    result_tref,
                    Value::from_pb_value(result_value),
                );
            }
        };
        let errno = match call_response.errno_option.unwrap() {
            | executor_pb::response::call::Errno_option::ErrnoSome(i) if i == 0 => {
                register_results();

                Some(i)
            },
            | executor_pb::response::call::Errno_option::ErrnoNone(_) => {
                register_results();

                None
            }
            | executor_pb::response::call::Errno_option::ErrnoSome(_) => {},
            | _ => todo!(),
        }

        self.store.push_snapshot(WasiSnapshot { errno });

        Ok(())
    }

    pub fn finish(self, spec: &witx::Document) -> FinalProg {
        let mut calls = Vec::new();
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();

        for call in self.calls {
            let func_spec = module_spec.func(&witx::Id::new(&call.func)).unwrap();
            let result_trefs = func_spec.unpack_expected_result();

            calls.push(r#final::Call {
                func:        call.func,
                params_post: call
                    .params_post
                    .into_iter()
                    .zip(func_spec.params.iter())
                    .map(|(v, param)| {
                        r#final::Value::from_stateful_value(param.tref.type_().as_ref(), v)
                    })
                    .collect(),
                results:     call
                    .results
                    .into_iter()
                    .zip(result_trefs.iter())
                    .map(|(v, tref)| r#final::Value::from_stateful_value(tref.type_().as_ref(), v))
                    .collect(),
                errno:       call.errno,
            });
        }

        FinalProg { calls }
    }

    fn get_func_pool(&self, spec: &witx::Document) -> Vec<Rc<witx::InterfaceFunc>> {
        let module = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();
        let mut func_pool = vec![];

        for func in module.funcs() {
            if FUNC_BLACKLIST.contains(func.name.as_str()) {
                continue;
            }

            func_pool.push(func);
        }

        func_pool
    }

    fn register_result_resource(
        &mut self,
        spec: &witx::Document,
        tref: &witx::TypeRef,
        value: Value,
    ) {
        if let Some(resource) = tref.resource(spec) {
            self.resource_ctx
                .new_resource(resource.name.as_str(), value.clone());
        }

        match (tref.type_().as_ref(), value) {
            | (witx::Type::Record(ty), Value::Record(value)) => {
                for (ty, value) in ty.members.iter().zip(value.0) {
                    self.register_result_resource(spec, &ty.tref, value);
                }
            },
            | (witx::Type::Record(_), Value::Bitflags(_)) => (),
            | (witx::Type::Variant(ty), Value::Variant(value)) => {
                let case = ty.cases.get(value.case_idx as usize).unwrap();

                if let (Some(payload_tref), Some(payload_value)) = (&case.tref, value.payload) {
                    self.register_result_resource(spec, payload_tref, *payload_value)
                }
            },
            | (witx::Type::Record(_), _) => unreachable!(),
            | (witx::Type::Variant(_), _) => unreachable!(),
            | (witx::Type::List(_), _) => unreachable!("result cannot be list"),
            | (witx::Type::Pointer(_), _) => unreachable!("result cannot be pointer"),
            | (witx::Type::ConstPointer(_), _) => unreachable!("result cannot be const pointer"),
            | (witx::Type::Handle(_), _) => (),
            | (witx::Type::Builtin(_), _) => (),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Value {
    Builtin(seed::BuiltinValue),
    Handle(u32),
    String(Vec<u8>),
    Bitflags(seed::BitflagsValue),
    Record(RecordValue),
    Pointer(Vec<Value>),
    ConstPointer(Vec<Value>),
    List(Vec<Value>),
    Variant(VariantValue),
}

impl Value {
    pub(crate) fn into_pb_value(self, ty: &witx::Type) -> executor_pb::Value {
        let which = match (ty, self.clone()) {
            | (witx::Type::Builtin(_), Value::Builtin(builtin)) => {
                let which = match builtin {
                    | seed::BuiltinValue::U8(i) => executor_pb::value::builtin::Which::U8(i.into()),
                    | seed::BuiltinValue::U32(i) => executor_pb::value::builtin::Which::U32(i),
                    | seed::BuiltinValue::U64(i) => executor_pb::value::builtin::Which::U64(i),
                    | seed::BuiltinValue::S64(i) => executor_pb::value::builtin::Which::S64(i),
                };

                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(which),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::Handle(handle)) => executor_pb::value::Which::Handle(handle),
            | (witx::Type::List(_), Value::String(bytes)) => {
                executor_pb::value::Which::String(bytes)
            },
            | (witx::Type::Record(record), Value::Bitflags(bitflags))
                if record.bitflags_repr().is_some() =>
            {
                let repr = match record.bitflags_repr().unwrap() {
                    | IntRepr::U8 => executor_pb::IntRepr::U8,
                    | IntRepr::U16 => executor_pb::IntRepr::U16,
                    | IntRepr::U32 => executor_pb::IntRepr::U32,
                    | IntRepr::U64 => executor_pb::IntRepr::U64,
                };

                executor_pb::value::Which::Bitflags(executor_pb::value::Bitflags {
                    repr:           protobuf::EnumOrUnknown::new(repr),
                    members:        bitflags
                        .0
                        .iter()
                        .cloned()
                        .map(executor_pb::value::bitflags::Member::from)
                        .collect(),
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Record(record_type), Value::Record(record)) => {
                executor_pb::value::Which::Record(executor_pb::value::Record {
                    members:        record
                        .0
                        .into_iter()
                        .zip(record_type.member_layout())
                        .zip(record_type.members.iter())
                        .map(|((member, member_layout), member_type)| {
                            executor_pb::value::record::Member {
                                name:           member_type.name.as_str().to_owned(),
                                value:          Some(
                                    member.into_pb_value(member_type.tref.type_().as_ref()),
                                )
                                .into(),
                                offset:         member_layout.offset as u32,
                                special_fields: Default::default(),
                            }
                        })
                        .collect(),
                    size:           record_type.mem_size() as u32,
                    special_fields: Default::default(),
                })
            },
            | (witx::Type::Variant(variant_type), Value::Variant(variant)) => {
                executor_pb::value::Which::Variant(Box::new(executor_pb::value::Variant {
                    case_idx:       variant.case_idx,
                    size:           variant_type.mem_size() as u32,
                    tag_repr:       protobuf::EnumOrUnknown::new(match variant_type.tag_repr {
                        | IntRepr::U8 => executor_pb::IntRepr::U8,
                        | IntRepr::U16 => executor_pb::IntRepr::U16,
                        | IntRepr::U32 => executor_pb::IntRepr::U32,
                        | IntRepr::U64 => executor_pb::IntRepr::U64,
                    }),
                    payload_offset: variant_type.payload_offset() as u32,
                    payload_option: Some(match variant.payload {
                        | Some(payload) => {
                            executor_pb::value::variant::Payload_option::PayloadSome(Box::new(
                                payload.into_pb_value(
                                    variant_type.cases[variant.case_idx as usize]
                                        .tref
                                        .as_ref()
                                        .unwrap()
                                        .type_()
                                        .as_ref(),
                                ),
                            ))
                        },
                        | None => executor_pb::value::variant::Payload_option::PayloadNone(
                            Default::default(),
                        ),
                    }),
                    special_fields: Default::default(),
                }))
            },
            | _ => panic!("spec and value mismatch: {:#?}", self),
        };

        executor_pb::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    pub(crate) fn from_pb_value(x: executor_pb::Value) -> Self {
        match x.which.unwrap() {
            | executor_pb::value::Which::Builtin(builtin) => {
                Self::Builtin(match builtin.which.unwrap() {
                    | executor_pb::value::builtin::Which::U8(i) => seed::BuiltinValue::U8(i as u8),
                    | executor_pb::value::builtin::Which::U32(i) => seed::BuiltinValue::U32(i),
                    | executor_pb::value::builtin::Which::U64(i) => seed::BuiltinValue::U64(i),
                    | executor_pb::value::builtin::Which::S64(i) => seed::BuiltinValue::S64(i),
                    | _ => unreachable!(),
                })
            },
            | executor_pb::value::Which::String(string) => Self::String(string),
            | executor_pb::value::Which::Bitflags(bitflags) => {
                let mut members = Vec::with_capacity(bitflags.members.len());

                for member in bitflags.members {
                    members.push(seed::BitflagsMember {
                        name:  member.name,
                        value: member.value,
                    });
                }

                Self::Bitflags(seed::BitflagsValue(members))
            },
            | executor_pb::value::Which::Handle(handle) => Self::Handle(handle),
            | executor_pb::value::Which::Array(array) => {
                Self::List(array.items.into_iter().map(Self::from_pb_value).collect())
            },
            | executor_pb::value::Which::Record(record) => Self::Record(RecordValue(
                record
                    .members
                    .iter()
                    .map(|m| Self::from_pb_value(*m.value.0.clone().unwrap()))
                    .collect(),
            )),
            | executor_pb::value::Which::ConstPointer(array) => {
                Self::ConstPointer(array.items.into_iter().map(Self::from_pb_value).collect())
            },
            | executor_pb::value::Which::Pointer(items) => {
                Self::Pointer(items.items.into_iter().map(Self::from_pb_value).collect())
            },
            | executor_pb::value::Which::Variant(variant) => {
                let payload = match variant.payload_option.unwrap() {
                    | executor_pb::value::variant::Payload_option::PayloadNone(_) => None,
                    | executor_pb::value::variant::Payload_option::PayloadSome(payload) => {
                        Some(Box::new(Self::from_pb_value(*payload)))
                    },
                    | _ => unreachable!(),
                };

                Self::Variant(VariantValue {
                    case_idx: variant.case_idx,
                    payload,
                })
            },
            | _ => unreachable!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordValue(pub Vec<Value>);

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct VariantValue {
    pub case_idx: u64,
    pub payload:  Option<Box<Value>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Call {
    pub func:        String,
    pub errno:       Option<i32>,
    pub params_post: Vec<Value>,
    pub results:     Vec<Value>,
}
