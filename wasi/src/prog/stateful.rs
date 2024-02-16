use witx::IntRepr;

use super::seed;
use crate::{prog::r#final, FinalProg};

#[derive(Debug)]
pub struct Prog {
    pub(crate) calls: Vec<Call>,
}

impl Prog {
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
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum Value {
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
