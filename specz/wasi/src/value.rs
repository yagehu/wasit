use crate::WasiType;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum WasiValue {
    Handle(u32),
    S64(i64),
    U64(u64),
    Flags(FlagsValue),
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
            | (_, Self::U64(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::U64(i)),
                    special_fields: Default::default(),
                },
            ),
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
            | (_, Self::Flags(_)) | (_, Self::Variant(_)) => unreachable!(),
        };

        wazzi_executor_pb_rust::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsValue {
    pub fields: Vec<bool>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantValue {
    pub case_idx: usize,
    pub payload:  Option<WasiValue>,
}
