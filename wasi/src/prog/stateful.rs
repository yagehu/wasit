use witx::IntRepr;

use crate::{
    prog::{
        r#final,
        seed::{BitflagsValue, BuiltinValue},
    },
    FinalProg,
};

#[derive(Debug)]
pub struct Prog {
    pub(crate) calls: Vec<Call>,
}

impl Prog {
    pub fn finish(self) -> FinalProg {
        let mut calls = Vec::new();

        for call in self.calls {
            calls.push(r#final::Call {
                func:    call.func,
                params:  vec![],
                results: call.results.into_iter().map(Into::into).collect(),
                errno:   call.errno,
            });
        }

        FinalProg { calls }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum Value {
    Builtin(BuiltinValue),
    Handle(u32),
    String(Vec<u8>),
    Bitflags(BitflagsValue),
}

impl Value {
    pub(crate) fn to_pb_value(self, ty: &witx::Type) -> executor_pb::Value {
        let which = match (ty, self.clone()) {
            | (witx::Type::Builtin(_), Value::Builtin(builtin)) => {
                let which = match builtin {
                    | BuiltinValue::U8(i) => executor_pb::value::builtin::Which::U8(i.into()),
                    | BuiltinValue::U32(i) => executor_pb::value::builtin::Which::U32(i),
                    | BuiltinValue::U64(i) => executor_pb::value::builtin::Which::U64(i),
                    | BuiltinValue::S64(i) => executor_pb::value::builtin::Which::S64(i),
                };

                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(which).into(),
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
            which:          Some(which).into(),
            special_fields: Default::default(),
        }
    }

    pub(crate) fn from_pb_value(x: executor_pb::Value) -> Self {
        match x.which.unwrap() {
            | executor_pb::value::Which::Builtin(builtin) => {
                Self::Builtin(match builtin.which.unwrap() {
                    | executor_pb::value::builtin::Which::U8(i) => BuiltinValue::U8(i as u8),
                    | executor_pb::value::builtin::Which::U32(i) => BuiltinValue::U32(i),
                    | executor_pb::value::builtin::Which::U64(i) => BuiltinValue::U64(i),
                    | executor_pb::value::builtin::Which::S64(i) => BuiltinValue::S64(i),
                    | _ => unreachable!(),
                })
            },
            | executor_pb::value::Which::String(_) => todo!(),
            | executor_pb::value::Which::Bitflags(_) => todo!(),
            | executor_pb::value::Which::Handle(handle) => Self::Handle(handle),
            | executor_pb::value::Which::Array(_) => todo!(),
            | executor_pb::value::Which::Record(_) => todo!(),
            | executor_pb::value::Which::ConstPointer(_) => todo!(),
            | executor_pb::value::Which::Pointer(_) => todo!(),
            | executor_pb::value::Which::Variant(_) => todo!(),
            | _ => unreachable!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Call {
    pub func:    String,
    pub errno:   Option<i32>,
    pub results: Vec<Value>,
}
