use serde::{Deserialize, Serialize};

use super::{seed, stateful};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FinalProg {
    pub calls: Vec<Call>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func:    String,
    pub params:  Vec<Value>,
    pub results: Vec<Value>,
    pub errno:   Option<i32>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Builtin(seed::BuiltinValue),
    Bitflags(seed::BitflagsValue),
    Handle(u32),
    String(seed::StringValue),
    Record(RecordValue),
    Variant(VariantValue),
}

impl Value {
    pub(crate) fn from_stateful_value(ty: &witx::Type, x: stateful::Value) -> Self {
        match (ty, x) {
            | (_, stateful::Value::Builtin(builtin)) => Self::Builtin(builtin),
            | (_, stateful::Value::Handle(handle)) => Self::Handle(handle),
            | (_, stateful::Value::String(string)) => Self::String(seed::StringValue::from(string)),
            | (_, stateful::Value::Bitflags(bitflags)) => Self::Bitflags(bitflags),
            | (witx::Type::Record(record_type), stateful::Value::Record(record)) => {
                let mut members = Vec::with_capacity(record.0.len());

                for (member_type, member) in record_type.members.iter().zip(record.0.iter()) {
                    members.push(RecordMemberValue {
                        name:  member_type.name.as_str().to_owned(),
                        value: Self::from_stateful_value(
                            member_type.tref.type_().as_ref(),
                            member.to_owned(),
                        ),
                    });
                }

                Self::Record(RecordValue(members))
            },
            | (_, stateful::Value::Record(_)) => panic!(),
            | (witx::Type::Variant(variant_type), stateful::Value::Variant(variant)) => {
                let case = variant_type.cases.get(variant.case_idx as usize).unwrap();

                Self::Variant(VariantValue {
                    name:    case.name.as_str().to_owned(),
                    payload: match variant.payload {
                        | Some(payload) => Some(Box::new(Self::from_stateful_value(
                            case.tref.as_ref().unwrap().type_().as_ref(),
                            *payload,
                        ))),
                        | None => None,
                    },
                })
            },
            | (_, stateful::Value::Variant(_)) => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordValue(pub Vec<RecordMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordMemberValue {
    pub name:  String,
    pub value: Value,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name:    String,
    pub payload: Option<Box<Value>>,
}
