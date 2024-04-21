use serde::{Deserialize, Serialize};
use wazzi_spec::package::{Defvaltype, Interface, Valtype};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ValueMeta {
    pub value:    Value,
    pub resource: Option<ResourceMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ResourceMeta {
    pub id:   u64,
    pub name: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    S64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Handle(u32),

    // Container types
    Record(RecordValue),
    Variant(Box<VariantValue>),
    List(Vec<ValueMeta>),

    Flags(FlagsValue),
    String(StringValue),
}

impl ValueMeta {
    pub fn zero_value_from_spec(interface: &Interface, valtype: &Valtype) -> Self {
        let def = interface.resolve_valtype(valtype).unwrap();

        Self {
            resource: None,
            value:    match def {
                | Defvaltype::S64 => Value::S64(0),
                | Defvaltype::U8 => Value::U8(0),
                | Defvaltype::U16 => Value::U16(0),
                | Defvaltype::U32 => Value::U32(0),
                | Defvaltype::U64 => Value::U64(0),
                | Defvaltype::List(_) => todo!(),
                | Defvaltype::Record(record) => Value::Record(RecordValue {
                    members: record
                        .members
                        .iter()
                        .map(|member| Self::zero_value_from_spec(interface, &member.ty))
                        .collect(),
                }),
                | Defvaltype::Variant(variant) => {
                    let case = &variant.cases[0];

                    Value::Variant(Box::new(VariantValue {
                        case_idx:  0,
                        case_name: case.name.clone(),
                        payload:   case
                            .payload
                            .as_ref()
                            .map(|valtype| Self::zero_value_from_spec(interface, valtype)),
                    }))
                },
                | Defvaltype::Handle => Value::Handle(0),
                | Defvaltype::Flags(flags) => Value::Flags(FlagsValue {
                    members: flags
                        .members
                        .iter()
                        .map(|member| FlagsMember {
                            name:  member.clone(),
                            value: false,
                        })
                        .collect(),
                }),
                | Defvaltype::Tuple(_) => todo!(),
                | Defvaltype::Result(_) => todo!(),
                | Defvaltype::String => todo!(),
            },
        }
    }

    pub fn into_pb(self, interface: &Interface, def: &Defvaltype) -> executor_pb::Value {
        let which = match (def, self.value) {
            | (_, Value::S64(i)) => {
                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(executor_pb::value::builtin::Which::S64(i.into())),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::U8(i)) => {
                executor_pb::value::Which::Builtin(executor_pb::value::Builtin {
                    which:          Some(executor_pb::value::builtin::Which::U8(i.into())),
                    special_fields: Default::default(),
                })
            },
            | (_, Value::U16(_i)) => todo!(),
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
                    size:           def.mem_size(interface),
                    special_fields: Default::default(),
                })
            },
            | (Defvaltype::Variant(variant_type), Value::Variant(variant)) => {
                executor_pb::value::Which::Variant(Box::new(executor_pb::value::Variant {
                    case_idx:       variant.case_idx.into(),
                    size:           variant_type.mem_size(interface),
                    tag_repr:       repr_to_pb(variant_type.tag_repr).into(),
                    payload_offset: variant_type.payload_offset(interface),
                    payload_option: Some(match variant.payload {
                        | Some(payload) => {
                            executor_pb::value::variant::Payload_option::PayloadSome(Box::new(
                                payload.into_pb(
                                    interface,
                                    &interface
                                        .resolve_valtype(
                                            variant_type.cases[variant.case_idx as usize]
                                                .payload
                                                .as_ref()
                                                .unwrap(),
                                        )
                                        .unwrap(),
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
            | (Defvaltype::List(list_type), Value::List(list)) => {
                let item_def = interface.resolve_valtype(&list_type.element).unwrap();

                executor_pb::value::Which::Array(executor_pb::value::Array {
                    items:          list
                        .into_iter()
                        .map(|item| item.into_pb(interface, &item_def))
                        .collect(),
                    item_size:      item_def.mem_size(interface),
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
            | (_, Value::Record(_))
            | (_, Value::Variant(_))
            | (_, Value::List(_))
            | (_, Value::Flags(_)) => panic!(),
        };

        executor_pb::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    pub fn from_pb(
        x: executor_pb::Value,
        interface: &Interface,
        valtype: &Valtype,
        before: &ValueMeta,
    ) -> Self {
        let def = interface.resolve_valtype(valtype).unwrap();

        Self {
            resource: before.resource.clone(),
            value:    match (def, x.which.unwrap(), &before.value) {
                | (_, executor_pb::value::Which::Builtin(builtin), _) => {
                    match builtin.which.unwrap() {
                        | executor_pb::value::builtin::Which::Char(_) => todo!(),
                        | executor_pb::value::builtin::Which::U8(i) => Value::U8(i as u8),
                        | executor_pb::value::builtin::Which::U32(i) => Value::U32(i),
                        | executor_pb::value::builtin::Which::U64(i) => Value::U64(i),
                        | executor_pb::value::builtin::Which::S64(i) => Value::S64(i),
                        | _ => todo!(),
                    }
                },
                | (_, executor_pb::value::Which::String(string), _) => {
                    Value::String(StringValue::from(string))
                },
                | (_, executor_pb::value::Which::Bitflags(flags), _) => Value::Flags(FlagsValue {
                    members: flags
                        .members
                        .into_iter()
                        .map(|member| FlagsMember {
                            name:  member.name,
                            value: member.value,
                        })
                        .collect(),
                }),
                | (_, executor_pb::value::Which::Handle(handle), _) => Value::Handle(handle),
                | (
                    Defvaltype::List(list),
                    executor_pb::value::Which::Array(array),
                    Value::List(before_items),
                ) => Value::List(
                    array
                        .items
                        .into_iter()
                        .zip(before_items)
                        .map(|(item, before)| {
                            Self::from_pb(item, interface, &list.element, &before)
                        })
                        .collect(),
                ),
                | (
                    Defvaltype::Record(record_type),
                    executor_pb::value::Which::Record(record),
                    Value::Record(before_record),
                ) => Value::Record(RecordValue {
                    members: record_type
                        .members
                        .into_iter()
                        .zip(record.members)
                        .zip(before_record.members.iter())
                        .map(|((member_type, member), before_member)| {
                            Self::from_pb(
                                *member.value.0.unwrap(),
                                interface,
                                &member_type.ty,
                                before_member,
                            )
                        })
                        .collect(),
                }),
                | (_, executor_pb::value::Which::ConstPointer(_), _) => todo!(),
                | (_, executor_pb::value::Which::Pointer(_), _) => todo!(),
                | (
                    Defvaltype::Variant(variant_type),
                    executor_pb::value::Which::Variant(variant),
                    Value::Variant(before_variant),
                ) => {
                    let case = &variant_type.cases[variant.case_idx as usize];

                    Value::Variant(Box::new(VariantValue {
                        case_idx:  variant.case_idx as u32,
                        case_name: case.name.clone(),
                        payload:   match variant.payload_option.unwrap() {
                            | executor_pb::value::variant::Payload_option::PayloadSome(payload) => {
                                Some(Self::from_pb(
                                    *payload,
                                    interface,
                                    case.payload.as_ref().unwrap(),
                                    before_variant.payload.as_ref().unwrap(),
                                ))
                            },
                            | executor_pb::value::variant::Payload_option::PayloadNone(_) => None,
                            | _ => unreachable!(),
                        },
                    }))
                },
                | _ => todo!(),
            },
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordValue {
    pub members: Vec<ValueMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub case_idx:  u32,
    pub case_name: String,
    pub payload:   Option<ValueMeta>,
}

impl VariantValue {
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

fn repr_to_pb(repr: wazzi_spec::package::IntRepr) -> executor_pb::IntRepr {
    match repr {
        | wazzi_spec::package::IntRepr::U8 => executor_pb::IntRepr::U8,
        | wazzi_spec::package::IntRepr::U16 => executor_pb::IntRepr::U16,
        | wazzi_spec::package::IntRepr::U32 => executor_pb::IntRepr::U32,
        | wazzi_spec::package::IntRepr::U64 => executor_pb::IntRepr::U64,
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
