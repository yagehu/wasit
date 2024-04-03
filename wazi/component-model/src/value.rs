use serde::{Deserialize, Serialize};
use wazzi_spec::package::{Defvaltype, Interface};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    U8(u8),
    U32(u32),
    U64(u64),
    Handle(u32),

    // Container types
    Record(RecordValue),
    List(Vec<Value>),

    Flags(FlagsValue),
    String(StringValue),
}

impl Value {
    pub fn zero_value_from_spec(interface: &Interface, def: &Defvaltype) -> Self {
        match def {
            | Defvaltype::U8 => Self::U8(0),
            | Defvaltype::U32 => Self::U32(0),
            | Defvaltype::U64 => Self::U64(0),
            | Defvaltype::List(_) => todo!(),
            | Defvaltype::Record(record) => Self::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .map(|member| {
                        Self::zero_value_from_spec(
                            interface,
                            &interface.resolve_valtype(&member.ty).unwrap(),
                        )
                    })
                    .collect(),
            }),
            | Defvaltype::Variant(_) => todo!(),
            | Defvaltype::Handle => Self::Handle(0),
            | Defvaltype::Flags(_) => todo!(),
            | Defvaltype::Tuple(_) => todo!(),
            | Defvaltype::Result(_) => todo!(),
            | Defvaltype::String => todo!(),
        }
    }

    pub fn into_pb(self, interface: &Interface, def: &Defvaltype) -> executor_pb::Value {
        let which = match (def, self) {
            | (_, Value::U8(i)) => {
                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(executor_pb::value::builtin::Which::U8(i.into())),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::U32(i)) => {
                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(executor_pb::value::builtin::Which::U32(i)),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::U64(i)) => {
                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(executor_pb::value::builtin::Which::U64(i)),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::Handle(handle)) => executor_pb::value::Which::Handle(handle),
            | (Defvaltype::Record(record_type), Value::Record(record)) => {
                executor_pb::value::Which::Record(executor_pb::value::Record {
                    members:        record_type
                        .members
                        .iter()
                        .zip(record_type.member_layout(interface))
                        .zip(record.members)
                        .map(|((member, member_layout), member_value)| {
                            executor_pb::value::record::Member {
                                name:           member.name.clone(),
                                value:          Some(member_value.into_pb(
                                    interface,
                                    &interface.resolve_valtype(&member.ty).unwrap(),
                                ))
                                .into(),
                                offset:         member_layout.offset,
                                special_fields: Default::default(),
                            }
                        })
                        .collect(),
                    size:           def.mem_size(),
                    special_fields: Default::default(),
                })
            },
            | (Defvaltype::List(list_type), Value::List(list)) => {
                let item_def = interface.resolve_valtype(&list_type.element).unwrap();

                executor_pb::value::Which::Array(executor_pb::value::Array {
                    items:          list
                        .into_iter()
                        .map(|item| item.into_pb(interface, &item_def))
                        .collect(),
                    item_size:      item_def.mem_size(),
                    special_fields: Default::default(),
                })
            },
            | (Defvaltype::Flags(flags_type), Value::Flags(flags)) => {
                executor_pb::value::Which::Bitflags(executor_pb::value::Bitflags {
                    repr:           repr_to_pb(flags_type.repr).into(),
                    members:        flags
                        .members
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
            | (_, Value::String(string)) => executor_pb::value::Which::String(string.into()),
            | (_, Value::Record(_)) | (_, Value::List(_)) | (_, Value::Flags(_)) => panic!(),
        };

        executor_pb::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    pub fn from_pb(x: executor_pb::Value) -> Self {
        match x.which.unwrap() {
            | executor_pb::value::Which::Builtin(builtin) => match builtin.which.unwrap() {
                | executor_pb::value::builtin::Which::Char(_) => todo!(),
                | executor_pb::value::builtin::Which::U8(_) => todo!(),
                | executor_pb::value::builtin::Which::U32(i) => Self::U32(i),
                | executor_pb::value::builtin::Which::U64(i) => Self::U64(i),
                | executor_pb::value::builtin::Which::S64(_) => todo!(),
                | _ => todo!(),
            },
            | executor_pb::value::Which::String(string) => Self::String(StringValue::from(string)),
            | executor_pb::value::Which::Bitflags(flags) => Self::Flags(FlagsValue {
                members: flags
                    .members
                    .into_iter()
                    .map(|member| FlagsMember {
                        name:  member.name,
                        value: member.value,
                    })
                    .collect(),
            }),
            | executor_pb::value::Which::Handle(handle) => Self::Handle(handle),
            | executor_pb::value::Which::Array(_) => todo!(),
            | executor_pb::value::Which::Record(_) => todo!(),
            | executor_pb::value::Which::ConstPointer(_) => todo!(),
            | executor_pb::value::Which::Pointer(_) => todo!(),
            | executor_pb::value::Which::Variant(_) => todo!(),
            | _ => todo!(),
        }
    }

    pub fn alignment(&self) -> u32 {
        match self {
            | Value::U8(_) => 1,
            | Value::U32(_) => 4,
            | Value::U64(_) => 8,
            | Value::Handle(_) => 4,
            | Value::Record(record) => record.alignment(),
            | Value::List(_) => 4,
            | Value::Flags(_) => todo!(),
            | Value::String(_) => 4,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordValue {
    pub members: Vec<Value>,
}

impl RecordValue {
    pub fn alignment(&self) -> u32 {
        self.members.iter().map(Value::alignment).max().unwrap_or(1)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FlagsValue {
    pub members: Vec<FlagsMember>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FlagsMember {
    pub name:  String,
    pub value: bool,
}

fn repr_to_pb(repr: wazzi_spec::package::Repr) -> executor_pb::IntRepr {
    match repr {
        | wazzi_spec::package::Repr::U16 => executor_pb::IntRepr::U16,
        | wazzi_spec::package::Repr::U32 => executor_pb::IntRepr::U32,
        | wazzi_spec::package::Repr::U64 => executor_pb::IntRepr::U64,
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl From<StringValue> for Vec<u8> {
    fn from(value: StringValue) -> Self {
        match value {
            | StringValue::Utf8(s) => s.into_bytes(),
            | StringValue::Bytes(bytes) => bytes,
        }
    }
}

impl From<Vec<u8>> for StringValue {
    fn from(value: Vec<u8>) -> Self {
        match String::from_utf8(value) {
            | Ok(s) => Self::Utf8(s),
            | Err(err) => Self::Bytes(err.into_bytes()),
        }
    }
}
