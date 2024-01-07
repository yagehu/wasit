use witx::TypeRef;

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
        | witx::Type::Record(_) => todo!(),
        | witx::Type::Variant(_) => todo!(),
        | witx::Type::Handle(_) => type_builder.reborrow().set_handle(()),
        | witx::Type::List(item_tref) => {
            if let witx::Type::Builtin(witx::BuiltinType::Char) = item_tref.type_().as_ref() {
                type_builder.set_string(());

                return;
            }

            unimplemented!("{:#?}", item_tref);
        },
        | witx::Type::Pointer(_) => todo!(),
        | witx::Type::ConstPointer(_) => todo!(),
        | witx::Type::Builtin(_) => todo!(),
    }
}
