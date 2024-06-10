use serde::{Deserialize, Serialize};

use crate::WasiType;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub enum WasiValue {
    Handle(u32),
    S64(i64),
    U8(u8),
    U32(u32),
    U64(u64),
    Record(RecordValue),
    Flags(FlagsValue),
    List(ListValue),
    String(Vec<u8>),
    Variant(Box<VariantValue>),
}

impl WasiValue {
    pub fn into_pb(self, ty: &WasiType) -> wazzi_executor_pb_rust::Value {
        let which = match (ty, self) {
            | (_, Self::Handle(handle)) => wazzi_executor_pb_rust::value::Which::Handle(handle),
            | (_, Self::S64(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::S64(i)),
                    special_fields: Default::default(),
                },
            ),
            | (_, Self::U8(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::U8(i.into())),
                    special_fields: Default::default(),
                },
            ),
            | (_, Self::U32(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::U32(i)),
                    special_fields: Default::default(),
                },
            ),
            | (_, Self::U64(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::U64(i)),
                    special_fields: Default::default(),
                },
            ),
            | (WasiType::Record(record_type), Self::Record(record)) => {
                wazzi_executor_pb_rust::value::Which::Record(wazzi_executor_pb_rust::value::Record {
                    members: record
                        .members
                        .into_iter()
                        .zip(record_type.members.iter())
                        .zip(record_type.member_layout())
                        .map(|((value, member), member_layout)| {
                            wazzi_executor_pb_rust::value::record::Member {
                                name: member.name.clone(),
                                value: Some(value.into_pb(&member.ty.wasi)).into(),
                                offset: member_layout.offset,
                                special_fields: Default::default(),
                            }
                        })
                        .collect(),
                    size: record_type.mem_size(),
                    special_fields: Default::default(),
                })

            },
            | (WasiType::Flags(flags_type), Self::Flags(flags)) => {
                wazzi_executor_pb_rust::value::Which::Bitflags(
                    wazzi_executor_pb_rust::value::Bitflags {
                        repr:           wazzi_executor_pb_rust::IntRepr::from(flags_type.repr)
                            .into(),
                        members:        flags_type
                            .fields
                            .iter()
                            .zip(flags.fields)
                            .map(|(field_name, field)| {
                                wazzi_executor_pb_rust::value::bitflags::Member {
                                    name:           field_name.to_owned(),
                                    value:          field,
                                    special_fields: Default::default(),
                                }
                            })
                            .collect(),
                        special_fields: Default::default(),
                    },
                )
            },
            | (WasiType::List(list_type), Self::List(list)) => {
                let items = list.items.into_iter().map(|item| {
                    item.into_pb(&list_type.item.wasi)
                }).collect();

                wazzi_executor_pb_rust::value::Which::Array(wazzi_executor_pb_rust::value::Array { items, item_size: list_type.item.wasi.mem_size(), special_fields: Default::default() })
            },
            | (_, Self::String(string)) => wazzi_executor_pb_rust::value::Which::String(string),
            | (WasiType::Variant(variant_type), Self::Variant(variant)) => {
                wazzi_executor_pb_rust::value::Which::Variant(Box::new(
                    wazzi_executor_pb_rust::value::Variant {
                        case_idx:       variant.case_idx as u64,
                        size:           variant_type.mem_size(),
                        tag_repr:       wazzi_executor_pb_rust::IntRepr::from(
                            variant_type.tag_repr,
                        )
                        .into(),
                        payload_offset: variant_type.payload_offset(),
                        payload_option: Some(
                            match &variant_type.cases.get(variant.case_idx).unwrap().payload {
                                | Some(payload) => wazzi_executor_pb_rust::value::variant::Payload_option::PayloadSome(
                                    Box::new(variant.payload.unwrap().into_pb(&payload.wasi))
                                ),
                                | None => wazzi_executor_pb_rust::value::variant::Payload_option::PayloadNone(Default::default()),
                            },
                        ),
                        special_fields: Default::default(),
                    },
                ))
            },
            | (_, Self::Record(_)) | (_, Self::Flags(_)) | (_, Self::List(_)) | (_, Self::Variant(_)) => unreachable!(),
        };

        wazzi_executor_pb_rust::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    pub fn from_pb(ty: &WasiType, value: wazzi_executor_pb_rust::Value) -> Self {
        match (ty, value.which.unwrap()) {
            | (_, wazzi_executor_pb_rust::value::Which::Handle(handle)) => Self::Handle(handle),
            | (_, wazzi_executor_pb_rust::value::Which::Builtin(builtin)) => {
                match builtin.which.unwrap() {
                    | wazzi_executor_pb_rust::value::builtin::Which::Char(_) => todo!(),
                    | wazzi_executor_pb_rust::value::builtin::Which::U8(_) => todo!(),
                    | wazzi_executor_pb_rust::value::builtin::Which::U32(i) => Self::U32(i),
                    | wazzi_executor_pb_rust::value::builtin::Which::U64(i) => Self::U64(i),
                    | wazzi_executor_pb_rust::value::builtin::Which::S64(i) => Self::S64(i),
                    | _ => todo!(),
                }
            },
            | (_, wazzi_executor_pb_rust::value::Which::Bitflags(flags)) => {
                Self::Flags(FlagsValue {
                    fields: flags
                        .members
                        .into_iter()
                        .map(|member| member.value)
                        .collect(),
                })
            },
            | (_, wazzi_executor_pb_rust::value::Which::String(string)) => Self::String(string),
            | (
                WasiType::Variant(variant_type),
                wazzi_executor_pb_rust::value::Which::Variant(variant),
            ) => Self::Variant(Box::new(VariantValue {
                case_idx: variant.case_idx as usize,
                payload:  match variant.payload_option.unwrap() {
                    | wazzi_executor_pb_rust::value::variant::Payload_option::PayloadSome(p) => {
                        Some(Self::from_pb(
                            &variant_type
                                .cases
                                .get(variant.case_idx as usize)
                                .unwrap()
                                .payload
                                .as_ref()
                                .unwrap()
                                .wasi,
                            *p,
                        ))
                    },
                    | wazzi_executor_pb_rust::value::variant::Payload_option::PayloadNone(_) => {
                        None
                    },
                    | _ => todo!(),
                },
            })),
            | _ => unreachable!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct RecordValue {
    pub members: Vec<WasiValue>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct FlagsValue {
    pub fields: Vec<bool>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ListValue {
    pub items: Vec<WasiValue>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct VariantValue {
    pub case_idx: usize,
    pub payload:  Option<WasiValue>,
}
