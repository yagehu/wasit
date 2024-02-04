use witx::Layout;

use crate::{
    action::BuiltinValue,
    snapshot::{PureRecordMemeber, PureValue, PureVariant, ValueView},
};

fn to_int_repr(x: &witx::IntRepr) -> executor_pb::IntRepr {
    match x {
        | witx::IntRepr::U8 => executor_pb::IntRepr::INT_REPR_U8,
        | witx::IntRepr::U16 => executor_pb::IntRepr::INT_REPR_U16,
        | witx::IntRepr::U32 => executor_pb::IntRepr::INT_REPR_U32,
        | witx::IntRepr::U64 => executor_pb::IntRepr::INT_REPR_U64,
    }
}

pub fn to_type(ty: &witx::Type) -> executor_pb::Type {
    use executor_pb::type_::Which;

    let which = Some(match ty {
        | witx::Type::Record(record) if record.bitflags_repr().is_some() => {
            let members = record
                .members
                .iter()
                .map(|member| member.name.as_str().to_owned())
                .collect::<Vec<_>>();

            Which::Bitflags(executor_pb::type_::Bitflags {
                members,
                repr: protobuf::EnumOrUnknown::new(to_int_repr(&record.bitflags_repr().unwrap())),
                special_fields: protobuf::SpecialFields::new(),
            })
        },
        | witx::Type::Record(record) => Which::Record(executor_pb::type_::Record {
            members:        record
                .members
                .iter()
                .zip(record.member_layout().iter())
                .map(
                    |(member, member_layout)| executor_pb::type_::record::Member {
                        name:           member.name.as_str().to_owned().into_bytes(),
                        type_:          Some(to_type(member.tref.type_().as_ref())).into(),
                        offset:         member_layout.offset as u32,
                        special_fields: protobuf::SpecialFields::new(),
                    },
                )
                .collect(),
            size:           record.mem_size() as u32,
            special_fields: protobuf::SpecialFields::new(),
        }),
        | witx::Type::Variant(variant) => Which::Variant(executor_pb::type_::Variant {
            tag_repr:       protobuf::EnumOrUnknown::new(to_int_repr(&variant.tag_repr)),
            cases:          variant
                .cases
                .iter()
                .map(|case| executor_pb::type_::variant::Case {
                    name:           case.name.as_str().to_owned().into_bytes(),
                    optional_type:  case.tref.as_ref().map(|tref| {
                        executor_pb::type_::variant::case::Optional_type::Type(to_type(
                            tref.type_().as_ref(),
                        ))
                    }),
                    special_fields: protobuf::SpecialFields::new(),
                })
                .collect(),
            payload_offset: variant.payload_offset() as u32,
            size:           variant.mem_size() as u32,
            special_fields: protobuf::SpecialFields::new(),
        }),
        | witx::Type::Handle(_) => Which::Handle(Default::default()),
        | witx::Type::List(element)
            if matches!(
                element.type_().as_ref(),
                witx::Type::Builtin(witx::BuiltinType::Char)
            ) =>
        {
            Which::String(Default::default())
        },
        | witx::Type::List(element_tref) => Which::Array(executor_pb::type_::Array {
            type_:          Some(to_type(element_tref.type_().as_ref())).into(),
            item_size:      element_tref.mem_size() as u32,
            special_fields: protobuf::SpecialFields::new(),
        }),
        | witx::Type::Pointer(tref) => Which::Pointer(Box::new(to_type(tref.type_().as_ref()))),
        | witx::Type::ConstPointer(pointer) => {
            Which::ConstPointer(Box::new(to_type(pointer.type_().as_ref())))
        },
        | witx::Type::Builtin(builtin) => {
            let which = Some(match builtin {
                | witx::BuiltinType::Char => todo!(),
                | witx::BuiltinType::U8 { .. } => {
                    executor_pb::type_::builtin::Which::U8(Default::default())
                },
                | witx::BuiltinType::U16 => todo!(),
                | witx::BuiltinType::U32 { .. } => {
                    executor_pb::type_::builtin::Which::U32(Default::default())
                },
                | witx::BuiltinType::U64 => {
                    executor_pb::type_::builtin::Which::U64(Default::default())
                },
                | witx::BuiltinType::S8 => todo!(),
                | witx::BuiltinType::S16 => todo!(),
                | witx::BuiltinType::S32 => todo!(),
                | witx::BuiltinType::S64 => {
                    executor_pb::type_::builtin::Which::S64(Default::default())
                },
                | witx::BuiltinType::F32 => todo!(),
                | witx::BuiltinType::F64 => todo!(),
            });

            Which::Builtin(executor_pb::type_::Builtin {
                which,
                special_fields: protobuf::SpecialFields::new(),
            })
        },
    });

    executor_pb::Type {
        which,
        special_fields: protobuf::SpecialFields::new(),
    }
}

pub fn from_value_view(x: &executor_pb::ValueView) -> ValueView {
    let value = match x.content.as_ref().unwrap().which.as_ref().unwrap() {
        | executor_pb::pure_value::Which::Builtin(builtin) => {
            PureValue::Builtin(match builtin.which.as_ref().unwrap() {
                | &executor_pb::raw_value::builtin::Which::U8(i) => BuiltinValue::U8(i as u8),
                | &executor_pb::raw_value::builtin::Which::U32(i) => BuiltinValue::U32(i),
                | &executor_pb::raw_value::builtin::Which::U64(i) => BuiltinValue::U64(i),
                | &executor_pb::raw_value::builtin::Which::S64(i) => BuiltinValue::S64(i),
                | _ => todo!(),
            })
        },
        | &executor_pb::pure_value::Which::Handle(handle) => PureValue::Handle(handle),
        | executor_pb::pure_value::Which::List(_) => todo!(),
        | executor_pb::pure_value::Which::Record(record) => {
            let mut members = Vec::with_capacity(record.members.len());

            for member in &record.members {
                members.push(PureRecordMemeber {
                    name: String::from_utf8_lossy(&member.name).to_string(),
                    view: from_value_view(member.value.as_ref().unwrap()),
                });
            }

            PureValue::Record(members)
        },
        | executor_pb::pure_value::Which::Pointer(_) => todo!(),
        | executor_pb::pure_value::Which::Variant(variant) => PureValue::Variant(PureVariant {
            case_idx:  variant.case_idx,
            case_name: String::from_utf8_lossy(&variant.case_name).to_string(),
            payload:   variant
                .optional_payload
                .as_ref()
                .map(|payload| match payload {
                    | executor_pb::pure_value::variant::Optional_payload::Payload(payload) => {
                        Box::new(from_value_view(payload))
                    },
                    | _ => unreachable!(),
                }),
        }),
        | _ => todo!(),
    };

    ValueView {
        memory_offset: x.memory_offset,
        value,
    }
}
