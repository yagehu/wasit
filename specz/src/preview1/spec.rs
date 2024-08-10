use std::collections::{BTreeMap, HashMap, HashSet};

use arbitrary::{Arbitrary, Unstructured};
use idxspace::IndexSpace;
use serde::{Deserialize, Serialize};

use super::witx::{elang, slang};
use crate::Resource;

#[derive(PartialEq, Eq, Debug)]
pub struct Spec<'ctx> {
    pub interfaces: IndexSpace<String, Interface>,

    types:         IndexSpace<String, TypeDef>,
    encoded_types: HashMap<String, EncodedType<'ctx>>,
}

impl<'ctx> Spec<'ctx> {
    pub fn new(ctx: &'ctx z3::Context) -> Self {
        let encoded_types = [
            (
                "bool".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Bool,
                    name:     "bool".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "bool")
                        .variant(
                            "bool",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)))],
                        )
                        .finish(),
                },
            ),
            (
                "s64".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Wasi(WasiType::S64),
                    name:     "s64".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "s64")
                        .variant(
                            "s64",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                },
            ),
            (
                "u8".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Wasi(WasiType::U8),
                    name:     "u8".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "u8")
                        .variant(
                            "u8",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                },
            ),
            (
                "u16".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Wasi(WasiType::U16),
                    name:     "u16".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "u16")
                        .variant(
                            "u16",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                },
            ),
            (
                "u32".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Wasi(WasiType::U32),
                    name:     "u32".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "u32")
                        .variant(
                            "u32",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                },
            ),
            (
                "u64".to_string(),
                EncodedType {
                    kind:     EncodedTypeKind::Wasi(WasiType::U64),
                    name:     "u64".to_string(),
                    datatype: z3::DatatypeBuilder::new(ctx, "u64")
                        .variant(
                            "u64",
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                },
            ),
        ]
        .into_iter()
        .collect();

        Self {
            types: Default::default(),
            interfaces: Default::default(),
            encoded_types,
        }
    }

    pub fn get_type_def(&self, name: &str) -> Option<&TypeDef> {
        self.types.get_by_key(name)
    }

    pub fn insert_type_def(&mut self, ctx: &'ctx z3::Context, name: String, tdef: TypeDef) {
        let mut datatype_builder = z3::DatatypeBuilder::new(ctx, name.as_str());
        let kind = match &tdef.attributes {
            | Some(attrs) => {
                datatype_builder = datatype_builder.variant(
                    "attrs",
                    attrs
                        .iter()
                        .map(|(name, attr_tref)| {
                            let ty = self.get_encoded_type_by_tref(attr_tref).unwrap();

                            (
                                name.as_str(),
                                z3::DatatypeAccessor::Sort(ty.datatype.sort.clone()),
                            )
                        })
                        .collect(),
                );

                EncodedTypeKind::Resource(attrs.clone())
            },
            | None => {
                datatype_builder = match &tdef.wasi {
                    | WasiType::S64
                    | WasiType::U8
                    | WasiType::U16
                    | WasiType::U32
                    | WasiType::U64 => datatype_builder.variant(
                        "inner",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    ),
                    | WasiType::Handle => panic!(),
                    | WasiType::Flags(flags) => datatype_builder.variant(
                        &name,
                        flags
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    field.as_str(),
                                    z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)),
                                )
                            })
                            .collect(),
                    ),
                    | WasiType::Variant(variant) => {
                        for case in &variant.cases {
                            let datatype_accessor = match &case.payload {
                                | Some(tref) => self.tref_to_datatype_accessor(tref).unwrap(),
                                | None => z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)),
                            };

                            datatype_builder = datatype_builder
                                .variant(&case.name, vec![("payload", datatype_accessor)]);
                        }

                        datatype_builder
                    },
                    | WasiType::Record(record) => datatype_builder.variant(
                        "inner",
                        record
                            .members
                            .iter()
                            .map(|member| -> Option<_> {
                                Some((
                                    member.name.as_str(),
                                    self.tref_to_datatype_accessor(&member.tref)?,
                                ))
                            })
                            .collect::<Option<_>>()
                            .unwrap(),
                    ),
                    | WasiType::String => datatype_builder.variant(
                        "inner",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
                    ),
                    | WasiType::List(_) => todo!(),
                };

                EncodedTypeKind::Wasi(tdef.wasi.clone())
            },
        };

        self.types.push(name.clone(), tdef);
        self.encoded_types.insert(
            name.clone(),
            EncodedType {
                kind,
                name,
                datatype: datatype_builder.finish(),
            },
        );
    }

    pub fn get_encoded_type_by_tref(&self, tref: &TypeRef) -> Option<&EncodedType<'ctx>> {
        match tref {
            | TypeRef::Named(name) => self.encoded_types.get(name),
            | TypeRef::Anonymous(_) => todo!(),
        }
    }

    fn tref_to_datatype_accessor(&self, tref: &TypeRef) -> Option<z3::DatatypeAccessor<'ctx>> {
        Some(z3::DatatypeAccessor::Sort(match tref {
            | TypeRef::Named(name) => self.encoded_types.get(name)?.datatype.sort.clone(),
            | TypeRef::Anonymous(_) => todo!(),
        }))
    }

    fn encode_anonymous_wasi_type(
        &self,
        ctx: &'ctx z3::Context,
        wasi_type: &WasiType,
    ) -> EncodedType<'ctx> {
        match wasi_type {
            | WasiType::S64 => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "s64".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "s64")
                    .variant(
                        "s64",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::U8 => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "u8".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "u8")
                    .variant(
                        "u8",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::U16 => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "u16".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "u16")
                    .variant(
                        "u16",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::U32 => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "u32".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "u32")
                    .variant(
                        "u32",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::U64 => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "u64".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "u64")
                    .variant(
                        "u64",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::Handle => EncodedType {
                kind:     EncodedTypeKind::Int,
                name:     "handle".to_string(),
                datatype: z3::DatatypeBuilder::new(ctx, "handle")
                    .variant(
                        "variant",
                        vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                    )
                    .finish(),
            },
            | WasiType::Flags(_) => todo!(),
            | WasiType::Variant(_) => todo!(),
            | WasiType::Record(_) => todo!(),
            | WasiType::String => todo!(),
            | WasiType::List(_) => todo!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum EncodedTypeKind {
    Bool,
    Int,
    Wasi(WasiType),
    Resource(BTreeMap<String, TypeRef>),
}

#[derive(Debug)]
pub struct EncodedType<'ctx> {
    pub kind: EncodedTypeKind,

    name:     String,
    datatype: z3::DatatypeSort<'ctx>,
}

impl<'ctx> EncodedType<'ctx> {
    pub fn attr_get<'spec>(
        &self,
        spec: &'spec Spec<'ctx>,
        value: &z3::ast::Dynamic<'ctx>,
        attr: &str,
    ) -> Option<(z3::ast::Dynamic<'ctx>, &'spec EncodedType<'ctx>)> {
        let attrs = match &self.kind {
            | EncodedTypeKind::Resource(attrs) => attrs,
            | _ => return None,
        };
        let (attr_idx, (_, tref)) = attrs
            .iter()
            .enumerate()
            .find(|&(_i, (attr_name, _attr_tref))| attr_name == attr)?;

        Some((
            self.datatype
                .variants
                .first()
                .unwrap()
                .accessors
                .get(attr_idx)
                .unwrap()
                .apply(&[value]),
            spec.get_encoded_type_by_tref(tref).unwrap(),
        ))
    }

    pub fn flags_get<'spec>(
        &self,
        spec: &'spec Spec<'ctx>,
        value: &z3::ast::Dynamic<'ctx>,
        field: &str,
    ) -> (z3::ast::Dynamic<'ctx>, &'spec EncodedType<'ctx>) {
        let flags = match &self.kind {
            | EncodedTypeKind::Bool => panic!(),
            | EncodedTypeKind::Int => panic!(),
            | EncodedTypeKind::Resource(_) => todo!(),
            | EncodedTypeKind::Wasi(WasiType::Flags(flags)) => flags,
            | EncodedTypeKind::Wasi(_) => panic!(),
        };
        let (idx, _field) = flags
            .fields
            .iter()
            .enumerate()
            .find(|&(_i, name)| name == field)
            .unwrap();

        (
            self.datatype
                .variants
                .first()
                .unwrap()
                .accessors
                .get(idx)
                .unwrap()
                .apply(&[value]),
            spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                .unwrap(),
        )
    }

    pub fn int_add(
        &self,
        ctx: &'ctx z3::Context,
        lhs: &z3::ast::Dynamic<'ctx>,
        rhs: &z3::ast::Dynamic<'ctx>,
    ) -> z3::ast::Dynamic<'ctx> {
        let lhs = self
            .datatype
            .variants
            .first()
            .unwrap()
            .accessors
            .first()
            .unwrap()
            .apply(&[lhs]);
        let rhs = self
            .datatype
            .variants
            .first()
            .unwrap()
            .accessors
            .first()
            .unwrap()
            .apply(&[rhs]);

        z3::ast::Dynamic::from_ast(&z3::ast::Int::add(
            ctx,
            &[&lhs.as_int().unwrap(), &rhs.as_int().unwrap()],
        ))
    }

    pub fn int_le(
        &self,
        lhs: &z3::ast::Dynamic<'ctx>,
        rhs: &z3::ast::Dynamic<'ctx>,
    ) -> z3::ast::Dynamic<'ctx> {
        let lhs = self
            .datatype
            .variants
            .first()
            .unwrap()
            .accessors
            .first()
            .unwrap()
            .apply(&[lhs]);
        let rhs = self
            .datatype
            .variants
            .first()
            .unwrap()
            .accessors
            .first()
            .unwrap()
            .apply(&[rhs]);

        z3::ast::Dynamic::from_ast(&lhs.as_int().unwrap().le(&rhs.as_int().unwrap()))
    }

    pub fn const_int_from_str(&self, ctx: &'ctx z3::Context, s: &str) -> z3::ast::Dynamic<'ctx> {
        match &self.kind {
            | EncodedTypeKind::Bool => panic!(),
            | EncodedTypeKind::Int => self
                .datatype
                .variants
                .first()
                .unwrap()
                .constructor
                .apply(&[&z3::ast::Int::from_str(ctx, s).unwrap()]),
            | EncodedTypeKind::Wasi(_) => todo!(),
            | EncodedTypeKind::Resource(_) => todo!(),
        }
    }

    pub fn const_variant(
        &self,
        ctx: &'ctx z3::Context,
        case_name: &str,
        payload: Option<z3::ast::Dynamic<'ctx>>,
    ) -> z3::ast::Dynamic<'ctx> {
        let payload = match payload {
            | Some(payload_value) => payload_value,
            | None => z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(ctx, true)),
        };
        let variant_type = match &self.kind {
            | EncodedTypeKind::Wasi(WasiType::Variant(variant)) => variant,
            | _ => panic!(),
        };
        let (i, _case) = variant_type
            .cases
            .iter()
            .enumerate()
            .find(|&(_i, case)| case.name == case_name)
            .unwrap();

        self.datatype
            .variants
            .get(i)
            .unwrap()
            .constructor
            .apply(&[&payload])
    }

    pub fn declare_const(&self, ctx: &'ctx z3::Context) -> z3::ast::Dynamic<'ctx> {
        match &self.kind {
            | EncodedTypeKind::Bool => todo!(),
            | EncodedTypeKind::Int => todo!(),
            | EncodedTypeKind::Resource(_attrs) => {
                z3::ast::Dynamic::fresh_const(ctx, &self.name, &self.datatype.sort)
            },
            | EncodedTypeKind::Wasi(_) => todo!(),
        }
    }

    pub fn wasi_value(&self, value: &z3::ast::Dynamic) -> WasiValue {
        let wasi_type = match &self.kind {
            | EncodedTypeKind::Wasi(wasi_type) => wasi_type,
            | _ => panic!(),
        };

        match wasi_type {
            | WasiType::S64 => WasiValue::S64(
                self.datatype
                    .variants
                    .first()
                    .unwrap()
                    .accessors
                    .first()
                    .unwrap()
                    .apply(&[value])
                    .as_int()
                    .unwrap()
                    .as_i64()
                    .unwrap(),
            ),
            | WasiType::U8 => WasiValue::U8(
                self.datatype
                    .variants
                    .first()
                    .unwrap()
                    .accessors
                    .first()
                    .unwrap()
                    .apply(&[value])
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap() as u8,
            ),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => WasiValue::U32(
                self.datatype
                    .variants
                    .first()
                    .unwrap()
                    .accessors
                    .first()
                    .unwrap()
                    .apply(&[value])
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap() as u32,
            ),
            | WasiType::U64 => WasiValue::U64(
                self.datatype
                    .variants
                    .first()
                    .unwrap()
                    .accessors
                    .first()
                    .unwrap()
                    .apply(&[value])
                    .as_int()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
            ),
            | WasiType::Handle => todo!(),
            | WasiType::Flags(_) => todo!(),
            | WasiType::Variant(_) => todo!(),
            | WasiType::Record(_) => todo!(),
            | WasiType::String => todo!(),
            | WasiType::List(_) => todo!(),
        }
    }

    pub fn encode_resource(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec<'ctx>,
        resource: &Resource,
    ) -> z3::ast::Dynamic<'ctx> {
        let attrs = match &self.kind {
            | EncodedTypeKind::Resource(attrs) => attrs,
            | _ => panic!(),
        };
        let wasi_value = WasiValue::Record(RecordValue {
            members: attrs
                .iter()
                .map(|(attr_name, _attr_tref)| {
                    resource.attributes.get(attr_name).unwrap().to_owned()
                })
                .collect::<Vec<_>>(),
        });

        self.encode_wasi_value(ctx, spec, &wasi_value)
    }

    pub fn encode_wasi_value(
        &self,
        ctx: &'ctx z3::Context,
        spec: &Spec<'ctx>,
        value: &WasiValue,
    ) -> z3::ast::Dynamic<'ctx> {
        let wasi_type = match &self.kind {
            | EncodedTypeKind::Wasi(wasi_type) => wasi_type,
            | _ => panic!(),
        };

        match (wasi_type, value) {
            | (WasiType::S64, &WasiValue::S64(i)) => self
                .datatype
                .variants
                .first()
                .unwrap()
                .constructor
                .apply(&[&z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(ctx, i))]),
            | (WasiType::S64, _) => panic!(),
            | (WasiType::U8, &WasiValue::U8(i)) => {
                self.datatype.variants.first().unwrap().constructor.apply(&[
                    &z3::ast::Dynamic::from_ast(&z3::ast::Int::from_u64(ctx, i.into())),
                ])
            },
            | (WasiType::U8, _) => panic!(),
            | (WasiType::U16, _) => panic!(),
            | (WasiType::U32, &WasiValue::U32(i)) => {
                self.datatype.variants.first().unwrap().constructor.apply(&[
                    &z3::ast::Dynamic::from_ast(&z3::ast::Int::from_u64(ctx, i.into())),
                ])
            },
            | (WasiType::U32, _) => panic!(),
            | (WasiType::U64, &WasiValue::U64(i)) => {
                self.datatype.variants.first().unwrap().constructor.apply(&[
                    &z3::ast::Dynamic::from_ast(&z3::ast::Int::from_u64(ctx, i.into())),
                ])
            },
            | (WasiType::U64, _) => panic!(),
            | (WasiType::Handle, &WasiValue::Handle(handle)) => {
                self.datatype.variants.first().unwrap().constructor.apply(&[
                    &z3::ast::Dynamic::from_ast(&z3::ast::Int::from_u64(ctx, handle.into())),
                ])
            },
            | (WasiType::Handle, _) => panic!(),
            | (WasiType::Flags(_flags_type), WasiValue::Flags(flags)) => {
                let fields = flags
                    .fields
                    .iter()
                    .map(|&b| z3::ast::Bool::from_bool(ctx, b))
                    .collect::<Vec<_>>();
                let fields = fields
                    .iter()
                    .map(|ast| ast as &dyn z3::ast::Ast)
                    .collect::<Vec<_>>();

                self.datatype
                    .variants
                    .first()
                    .unwrap()
                    .constructor
                    .apply(fields.as_slice())
            },
            | (WasiType::Flags(_), _) => panic!(),
            | (WasiType::Variant(variant_type), WasiValue::Variant(variant)) => {
                let payload = match &variant.payload {
                    | Some(value) => spec
                        .get_encoded_type_by_tref(
                            variant_type
                                .cases
                                .get(variant.case_idx)
                                .unwrap()
                                .payload
                                .as_ref()
                                .unwrap(),
                        )
                        .unwrap()
                        .encode_wasi_value(ctx, spec, &value),
                    | None => z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(ctx, true)),
                };

                self.datatype
                    .variants
                    .get(variant.case_idx)
                    .unwrap()
                    .constructor
                    .apply(&[&payload])
            },
            | (WasiType::Variant(_), _) => panic!(),
            | (WasiType::Record(record_type), WasiValue::Record(record)) => {
                let members = record_type
                    .members
                    .iter()
                    .zip(record.members.iter())
                    .map(|(member_type, member)| {
                        spec.get_encoded_type_by_tref(&member_type.tref)
                            .unwrap()
                            .encode_wasi_value(ctx, spec, member)
                    })
                    .collect::<Vec<_>>();

                self.datatype.variants.first().unwrap().constructor.apply(
                    members
                        .iter()
                        .map(|member| member as &dyn z3::ast::Ast)
                        .collect::<Vec<_>>()
                        .as_slice(),
                )
            },
            | (WasiType::Record(_), _) => panic!(),
            | (WasiType::String, _) => panic!(),
            | (WasiType::List(_list_type), WasiValue::List(_list)) => todo!(),
            | (WasiType::List(_), _) => panic!(),
        }
    }
}

impl PartialEq for EncodedType<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for EncodedType<'_> {
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TypeDef {
    pub name:       String,
    pub wasi:       WasiType,
    pub attributes: Option<BTreeMap<String, TypeRef>>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum TypeRef {
    Named(String),
    Anonymous(WasiType),
}

impl TypeRef {
    pub fn resource_type_def<'a>(&self, spec: &'a Spec) -> Option<&'a TypeDef> {
        match self {
            | TypeRef::Named(name) => spec.types.get_by_key(name),
            | TypeRef::Anonymous(_) => None,
        }
    }

    pub fn wasi_type<'a>(&'a self, spec: &'a Spec) -> &'a WasiType {
        match self {
            | TypeRef::Named(name) => &spec.types.get_by_key(name).unwrap().wasi,
            | TypeRef::Anonymous(wasi_type) => wasi_type,
        }
    }

    pub fn arbitrary_value(
        &self,
        spec: &Spec,
        u: &mut Unstructured,
        string_prefix: Option<&[u8]>,
    ) -> Result<WasiValue, arbitrary::Error> {
        self.wasi_type(spec).arbitrary_value(spec, u, string_prefix)
    }

    fn zero_value(&self, spec: &Spec) -> WasiValue {
        self.wasi_type(spec).zero_value(spec)
    }

    fn alignment(&self, spec: &Spec) -> u32 {
        self.wasi_type(spec).alignment(spec)
    }

    fn mem_size(&self, spec: &Spec) -> u32 {
        self.wasi_type(spec).mem_size(spec)
    }
}

#[derive(Clone, Debug)]
pub struct RestrictedString(Vec<u8>);

impl RestrictedString {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> Arbitrary<'a> for RestrictedString {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let len = u.choose_index(32)? + 1;
        let mut bytes = Vec::with_capacity(len);

        for _ in 0..len {
            bytes.push(*u.choose(&[b'.', b'/', b'a'])?);
        }

        Ok(Self(bytes))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum WasiType {
    S64,
    U8,
    U16,
    U32,
    U64,
    Handle,
    Flags(FlagsType),
    Variant(VariantType),
    Record(RecordType),
    String,
    List(Box<ListType>),
}

impl WasiType {
    pub fn flags(&self) -> Option<&FlagsType> {
        match self {
            | Self::Flags(flags) => Some(flags),
            | _ => None,
        }
    }

    pub fn variant(&self) -> Option<&VariantType> {
        match self {
            | Self::Variant(variant) => Some(variant),
            | _ => None,
        }
    }

    pub fn zero_value(&self, spec: &Spec) -> WasiValue {
        match self {
            | WasiType::S64 => WasiValue::S64(0),
            | WasiType::U8 => todo!(),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => todo!(),
            | WasiType::U64 => WasiValue::U64(0),
            | WasiType::Handle => WasiValue::Handle(0),
            | WasiType::Flags(flags) => WasiValue::Flags(FlagsValue {
                fields: flags.fields.iter().map(|_| false).collect(),
            }),
            | WasiType::Variant(variant) => WasiValue::Variant(Box::new(VariantValue {
                case_idx: 0,
                payload:  variant
                    .cases
                    .first()
                    .unwrap()
                    .payload
                    .as_ref()
                    .map(|payload| payload.zero_value(spec)),
            })),
            | WasiType::Record(_) => todo!(),
            | WasiType::String => WasiValue::String(Vec::new()),
            | WasiType::List(_) => todo!(),
        }
    }

    pub fn arbitrary_value(
        &self,
        spec: &Spec,
        u: &mut Unstructured,
        string_prefix: Option<&[u8]>,
    ) -> Result<WasiValue, arbitrary::Error> {
        Ok(match self {
            | WasiType::S64 => WasiValue::S64(u.arbitrary()?),
            | WasiType::U8 => WasiValue::U8(u.arbitrary()?),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => WasiValue::U32(u.arbitrary()?),
            | WasiType::U64 => WasiValue::U64(u.arbitrary()?),
            | WasiType::Handle => WasiValue::Handle(u.arbitrary()?),
            | WasiType::Flags(flags) => WasiValue::Flags(FlagsValue {
                fields: flags
                    .fields
                    .iter()
                    .map(|_f| u.arbitrary())
                    .collect::<Result<Vec<bool>, _>>()?,
            }),
            | WasiType::Variant(variant) => {
                let case_idx = u.choose_index(variant.cases.len())?;

                WasiValue::Variant(Box::new(VariantValue {
                    case_idx,
                    payload: variant
                        .cases
                        .get(case_idx)
                        .unwrap()
                        .payload
                        .as_ref()
                        .map(|t| t.arbitrary_value(spec, u, string_prefix))
                        .transpose()?,
                }))
            },
            | WasiType::Record(record) => WasiValue::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .map(|member| member.tref.arbitrary_value(spec, u, string_prefix))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            | WasiType::String => {
                let s: Vec<u8> = loop {
                    let s = u.arbitrary::<RestrictedString>()?;

                    if !s.is_empty() {
                        break s;
                    }

                    if u.is_empty() {
                        panic!("data exhausted");
                    }
                }
                .as_bytes()
                .to_vec();

                let mut string_prefix = string_prefix.unwrap_or_default().to_vec();

                if s.starts_with(&[47u8]) {
                    WasiValue::String(s)
                } else {
                    if !string_prefix.is_empty() {
                        string_prefix.push(b'/');
                    }

                    string_prefix.extend_from_slice(&s);

                    WasiValue::String(string_prefix)
                }
            },
            | WasiType::List(list) => {
                let len = u.choose_index(4)?;
                let mut items = Vec::with_capacity(len);

                for _i in 0..len {
                    items.push(list.item.arbitrary_value(spec, u, string_prefix)?);
                }

                WasiValue::List(ListValue { items })
            },
        })
    }

    pub fn mem_size(&self, spec: &Spec) -> u32 {
        match self {
            | Self::U8 => 1,
            | Self::U16 => 2,
            | Self::U32 => 4,
            | Self::S64 | Self::U64 => 8,
            | Self::List(_) => 8,
            | Self::Record(record) => record.mem_size(spec),
            | Self::Variant(variant) => variant.mem_size(spec),
            | Self::Handle => 4,
            | Self::Flags(flags) => flags.repr.mem_size(),
            | Self::String => todo!(),
        }
    }

    pub fn alignment(&self, spec: &Spec) -> u32 {
        match self {
            | Self::U8 => 1,
            | Self::U16 => 2,
            | Self::U32 => 4,
            | Self::S64 | Self::U64 => 8,
            | Self::List(_) => 4,
            | Self::Record(record) => record.alignment(spec),
            | Self::Variant(variant) => variant.alignment(spec),
            | Self::Handle => 4,
            | Self::Flags(flags) => flags.repr.alignment(),
            | Self::String => todo!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType {
    pub repr:   IntRepr,
    pub fields: Vec<String>,
}

impl FlagsType {
    pub fn value(&self, fields: HashSet<&str>) -> WasiValue {
        WasiValue::Flags(FlagsValue {
            fields: self
                .fields
                .iter()
                .map(|field| fields.contains(field.as_str()))
                .collect(),
        })
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum IntRepr {
    U8,
    U16,
    U32,
    U64,
}

impl IntRepr {
    pub fn alignment(&self) -> u32 {
        match self {
            | IntRepr::U8 => 1,
            | IntRepr::U16 => 2,
            | IntRepr::U32 => 4,
            | IntRepr::U64 => 8,
        }
    }

    pub fn mem_size(&self) -> u32 {
        match self {
            | IntRepr::U8 => 1,
            | IntRepr::U16 => 2,
            | IntRepr::U32 => 4,
            | IntRepr::U64 => 8,
        }
    }
}

impl From<IntRepr> for wazzi_executor_pb_rust::IntRepr {
    fn from(value: IntRepr) -> Self {
        match value {
            | IntRepr::U8 => Self::U8,
            | IntRepr::U16 => Self::U16,
            | IntRepr::U32 => Self::U32,
            | IntRepr::U64 => Self::U64,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantType {
    pub tag_repr: IntRepr,
    pub cases:    Vec<VariantCaseType>,
}

impl VariantType {
    pub fn value_from_name(
        &self,
        case_name: &str,
        payload: Option<WasiValue>,
    ) -> Option<WasiValue> {
        Some(WasiValue::Variant(Box::new(VariantValue {
            case_idx: self
                .cases
                .iter()
                .enumerate()
                .find(|(_, case)| case.name == case_name)
                .map(|(i, _)| i)?,
            payload,
        })))
    }

    pub fn alignment(&self, spec: &Spec) -> u32 {
        self.tag_repr.alignment().max(self.max_case_alignment(spec))
    }

    pub fn mem_size(&self, spec: &Spec) -> u32 {
        let mut size = self.tag_repr.mem_size();

        size = align_to(size, self.max_case_alignment(spec));
        size += self
            .cases
            .iter()
            .filter_map(|case| case.payload.as_ref())
            .map(|payload| payload.mem_size(spec))
            .max()
            .unwrap_or(0);

        align_to(size, self.alignment(spec))
    }

    pub fn payload_offset(&self, spec: &Spec) -> u32 {
        let size = self.tag_repr.mem_size();

        align_to(size, self.max_case_alignment(spec))
    }

    fn max_case_alignment(&self, spec: &Spec) -> u32 {
        self.cases
            .iter()
            .filter_map(|case| case.payload.as_ref())
            .map(|payload| payload.alignment(spec))
            .max()
            .unwrap_or(1)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantCaseType {
    pub name:    String,
    pub payload: Option<TypeRef>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordType {
    pub members: Vec<RecordMemberType>,
}

impl RecordType {
    pub fn mem_size(&self, spec: &Spec) -> u32 {
        let mut size: u32 = 0;
        let alignment = self.alignment(spec);

        for member in &self.members {
            let alignment = member.tref.alignment(spec);

            size = size.div_ceil(alignment) * alignment;
            size += member.tref.mem_size(spec);
        }

        size.div_ceil(alignment) * alignment
    }

    pub fn alignment(&self, spec: &Spec) -> u32 {
        self.members
            .iter()
            .map(|member| member.tref.alignment(spec))
            .max()
            .unwrap_or(1)
    }

    pub fn member_layout(&self, spec: &Spec) -> Vec<RecordMemberLayout> {
        let mut offset: u32 = 0;
        let mut layout = Vec::with_capacity(self.members.len());

        for member in &self.members {
            let alignment = member.tref.alignment(spec);

            offset = offset.div_ceil(alignment) * alignment;
            layout.push(RecordMemberLayout { offset });
            offset += member.tref.mem_size(spec);
        }

        layout
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberType {
    pub name: String,
    pub tref: TypeRef,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberLayout {
    pub offset: u32,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {
    pub item: TypeRef,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Interface {
    pub functions: BTreeMap<String, Function>,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            functions: Default::default(),
        }
    }
}

impl Default for Interface {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub name:           String,
    pub params:         Vec<FunctionParam>,
    pub results:        Vec<FunctionResult>,
    pub r#return:       Option<()>,
    pub input_contract: Option<slang::Term>,
    pub effects:        elang::Program,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name: String,
    pub tref: TypeRef,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionResult {
    pub name: String,
    pub tref: TypeRef,
}

fn align_to(ptr: u32, alignment: u32) -> u32 {
    ptr.div_ceil(alignment) * alignment
}

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
    pub fn into_pb(self, spec: &Spec, tref: &TypeRef) -> wazzi_executor_pb_rust::Value {
        let which = match (tref.wasi_type(spec), self) {
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
                        .zip(record_type.member_layout(spec))
                        .map(|((value, member), member_layout)| {
                            wazzi_executor_pb_rust::value::record::Member {
                                name: member.name.clone(),
                                value: Some(value.into_pb(spec, &member.tref)).into(),
                                offset: member_layout.offset,
                                special_fields: Default::default(),
                            }
                        })
                        .collect(),
                    size: record_type.mem_size(spec),
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
                    item.into_pb(spec, &list_type.item)
                }).collect();

                wazzi_executor_pb_rust::value::Which::Array(
                    wazzi_executor_pb_rust::value::Array {
                        items,
                        item_size: list_type.item.mem_size(spec),
                        special_fields: Default::default()
                    }
                )
            },
            | (_, Self::String(string)) => wazzi_executor_pb_rust::value::Which::String(string),
            | (WasiType::Variant(variant_type), Self::Variant(variant)) => {
                wazzi_executor_pb_rust::value::Which::Variant(Box::new(
                    wazzi_executor_pb_rust::value::Variant {
                        case_idx:       variant.case_idx as u64,
                        size:           variant_type.mem_size(spec),
                        tag_repr:       wazzi_executor_pb_rust::IntRepr::from(
                            variant_type.tag_repr,
                        )
                        .into(),
                        payload_offset: variant_type.payload_offset(spec),
                        payload_option: Some(
                            match &variant_type.cases.get(variant.case_idx).unwrap().payload {
                                | Some(payload) => wazzi_executor_pb_rust::value::variant::Payload_option::PayloadSome(
                                    Box::new(variant.payload.unwrap().into_pb(spec, &payload))
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

    pub fn from_pb(spec: &Spec, tref: &TypeRef, value: wazzi_executor_pb_rust::Value) -> Self {
        match (tref.wasi_type(spec), value.which.unwrap()) {
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
                            spec,
                            variant_type
                                .cases
                                .get(variant.case_idx as usize)
                                .unwrap()
                                .payload
                                .as_ref()
                                .unwrap(),
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
