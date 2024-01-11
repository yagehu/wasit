use witx::Layout;

pub(crate) fn from_witx_int_repr(x: &witx::IntRepr) -> wazzi_executor_capnp::type_::IntRepr {
    match x {
        | witx::IntRepr::U8 => wazzi_executor_capnp::type_::IntRepr::U8,
        | witx::IntRepr::U16 => wazzi_executor_capnp::type_::IntRepr::U16,
        | witx::IntRepr::U32 => wazzi_executor_capnp::type_::IntRepr::U32,
        | witx::IntRepr::U64 => wazzi_executor_capnp::type_::IntRepr::U64,
    }
}

pub(crate) fn build_type(
    r#type: &witx::Type,
    type_builder: &mut wazzi_executor_capnp::type_::Builder,
) {
    match r#type {
        | witx::Type::Record(record) if record.bitflags_repr().is_some() => {
            let mut bitflags_builder = type_builder.reborrow().init_bitflags();
            let mut members_builder = bitflags_builder
                .reborrow()
                .init_members(record.members.len() as u32);

            for (i, member) in record.members.iter().enumerate() {
                let mut member_builder = members_builder.reborrow().get(i as u32);
                let mut name_builder = member_builder
                    .reborrow()
                    .init_name(member.name.as_str().len() as u32);

                name_builder.push_str(member.name.as_str());
            }

            bitflags_builder
                .reborrow()
                .set_repr(from_witx_int_repr(&record.bitflags_repr().unwrap()));
        },
        | witx::Type::Record(record) => {
            let mut record_builder = type_builder.reborrow().init_record();

            record_builder.reborrow().set_size(record.mem_size() as u32);

            let mut members_builder = record_builder
                .reborrow()
                .init_members(record.members.len() as u32);

            for (i, (member, layout)) in record
                .members
                .iter()
                .zip(record.member_layout().iter())
                .enumerate()
            {
                let mut member_builder = members_builder.reborrow().get(i as u32);

                member_builder
                    .reborrow()
                    .init_name(member.name.as_str().len() as u32)
                    .push_str(member.name.as_str());
                member_builder.reborrow().set_offset(layout.offset as u32);

                let mut member_type_builder = member_builder.reborrow().init_type();

                build_type(member.tref.type_().as_ref(), &mut member_type_builder);
            }
        },
        | witx::Type::Variant(variant) => {
            let mut variant_builder = type_builder.reborrow().init_variant();

            variant_builder
                .reborrow()
                .set_payload_offset(variant.payload_offset() as u32);
            variant_builder
                .reborrow()
                .set_size(variant.mem_size() as u32);
            variant_builder
                .reborrow()
                .set_tag_repr(from_witx_int_repr(&variant.tag_repr));

            let mut cases_builder = variant_builder.init_cases(variant.cases.len() as u32);

            for (i, case_type) in variant.cases.iter().enumerate() {
                let mut case_builder = cases_builder.reborrow().get(i as u32);

                case_builder
                    .reborrow()
                    .init_name(case_type.name.as_str().len() as u32)
                    .push_str(case_type.name.as_str());

                let mut case_type_builder = case_builder.reborrow().init_type();

                match &case_type.tref {
                    | None => case_type_builder.reborrow().set_none(()),
                    | Some(tref) => build_type(
                        tref.type_().as_ref(),
                        &mut case_type_builder.reborrow().init_some(),
                    ),
                }
            }
        },
        | witx::Type::Handle(_) => type_builder.reborrow().set_handle(()),
        | witx::Type::List(item_tref) => {
            if let witx::Type::Builtin(witx::BuiltinType::Char) = item_tref.type_().as_ref() {
                type_builder.set_string(());

                return;
            }

            let mut array_builder = type_builder.reborrow().init_array();
            let mut item_builder = array_builder.reborrow().init_item();

            build_type(item_tref.type_().as_ref(), &mut item_builder);

            array_builder
                .reborrow()
                .set_item_size(item_tref.mem_size() as u32);
        },
        | witx::Type::Pointer(tref) => {
            let mut pointer_builder = type_builder.reborrow().init_pointer();

            build_type(tref.type_().as_ref(), &mut pointer_builder);
        },
        | witx::Type::ConstPointer(tref) => {
            let mut pointee_builder = type_builder.reborrow().init_const_pointer();

            build_type(tref.type_().as_ref(), &mut pointee_builder);
        },
        | witx::Type::Builtin(builtin) => {
            let mut builtin_builder = type_builder.reborrow().init_builtin();

            match builtin {
                | witx::BuiltinType::Char => builtin_builder.set_char(()),
                | witx::BuiltinType::U8 { .. } => builtin_builder.set_u8(()),
                | witx::BuiltinType::U16 => builtin_builder.set_u16(()),
                | witx::BuiltinType::U32 { .. } => builtin_builder.set_u32(()),
                | witx::BuiltinType::U64 => builtin_builder.set_u64(()),
                | witx::BuiltinType::S8 => builtin_builder.set_s8(()),
                | witx::BuiltinType::S16 => builtin_builder.set_s16(()),
                | witx::BuiltinType::S32 => builtin_builder.set_s32(()),
                | witx::BuiltinType::S64 => builtin_builder.set_s64(()),
                | witx::BuiltinType::F32 | witx::BuiltinType::F64 => unimplemented!(),
            }
        },
    }
}
