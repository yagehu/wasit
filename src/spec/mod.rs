pub mod witx;

use std::collections::{BTreeMap, HashSet};

use arbitrary::Unstructured;
use idxspace::IndexSpace;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use witx::slang;

#[derive(Debug)]
pub struct Spec {
    pub(crate) types:      IndexSpace<String, TypeDef>,
    pub(crate) interfaces: IndexSpace<String, Interface>,
}

impl Spec {
    fn new() -> Self {
        let mut types: IndexSpace<String, TypeDef> = Default::default();

        types.push(
            "s64".to_string(),
            TypeDef {
                name:  "s64".to_string(),
                wasi:  WasiType::S64,
                state: None,
            },
        );
        types.push(
            "u8".to_string(),
            TypeDef {
                name:  "u8".to_string(),
                wasi:  WasiType::U8,
                state: None,
            },
        );
        types.push(
            "u32".to_string(),
            TypeDef {
                name:  "u32".to_string(),
                wasi:  WasiType::U32,
                state: None,
            },
        );
        types.push(
            "u64".to_string(),
            TypeDef {
                name:  "u64".to_string(),
                wasi:  WasiType::U64,
                state: None,
            },
        );

        Self {
            types,
            interfaces: Default::default(),
        }
    }

    pub fn preview1() -> Result<Self, eyre::Error> {
        witx::preview1()
    }

    fn insert_type_def(&mut self, name: String, wasi: WasiType, state: Option<WasiType>) {
        self.types.push(name.clone(), TypeDef { name, wasi, state });
    }

    pub fn get_wasi_type(&self, name: &str) -> Option<WasiType> {
        let tdef = self.types.get_by_key(name)?;

        Some(match &tdef.state {
            | Some(state) => state.clone(),
            | None => tdef.wasi.clone(),
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Interface {
    pub(crate) functions: BTreeMap<String, Function>,
}

impl Interface {
    fn new() -> Self {
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
    pub name:                   String,
    pub params:                 Vec<FunctionParam>,
    pub results:                Vec<FunctionResult>,
    pub r#return:               Option<()>,
    pub(crate) input_contract:  Option<slang::Term>,
    pub(crate) output_contract: Option<slang::Term>,
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

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum TypeRef {
    Named(String),
    Anonymous(WasiType),
}

impl TypeRef {
    fn alignment(&self, spec: &Spec) -> u32 {
        self.resolve(spec).wasi.alignment(spec)
    }

    fn mem_size(&self, spec: &Spec) -> u32 {
        self.resolve(spec).wasi.mem_size(spec)
    }

    pub fn resolve<'ctx, 'spec>(&self, spec: &'spec Spec) -> &'spec TypeDef {
        match self {
            | Self::Named(name) => spec.types.get_by_key(name).unwrap(),
            | Self::Anonymous(wasi_type) => match wasi_type {
                | WasiType::S64 => spec.types.get_by_key("s64").unwrap(),
                | WasiType::U8 => spec.types.get_by_key("u8").unwrap(),
                | WasiType::U16 => spec.types.get_by_key("u16").unwrap(),
                | WasiType::U32 => spec.types.get_by_key("u32").unwrap(),
                | WasiType::U64 => spec.types.get_by_key("u64").unwrap(),
                | WasiType::Handle => spec.types.get_by_key("handle").unwrap(),
                | _ => panic!("{:?}", wasi_type),
            },
        }
    }

    fn resolve_wasi<'ctx, 'spec>(&self, spec: &'spec Spec) -> WasiType {
        match self {
            | Self::Named(name) => spec.types.get_by_key(name).unwrap().wasi.clone(),
            | Self::Anonymous(wasi_type) => wasi_type.to_owned(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TypeDef {
    pub name:  String,
    pub wasi:  WasiType,
    pub state: Option<WasiType>,
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
    Pointer(Box<PointerType>),
    String,
    List(Box<ListType>),
}

impl WasiType {
    pub fn zero_value(&self, spec: &Spec) -> WasiValue {
        match self {
            | WasiType::S64 => WasiValue::S64(0),
            | WasiType::U8 => WasiValue::U8(0),
            | WasiType::U16 => WasiValue::U16(0),
            | WasiType::U32 => WasiValue::U32(0),
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
                    .map(|payload| payload.resolve(spec).wasi.zero_value(spec)),
            })),
            | WasiType::Record(record) => WasiValue::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .map(|member| member.tref.resolve(spec).wasi.zero_value(spec))
                    .collect::<Vec<_>>(),
            }),
            | WasiType::Pointer(_pointer) => WasiValue::Pointer(PointerValue { items: vec![] }),
            | WasiType::String => WasiValue::String(Vec::new()),
            | WasiType::List(_list) => WasiValue::List(ListValue { items: vec![] }),
        }
    }

    pub(crate) fn arbitrary_value(
        &self,
        spec: &Spec,
        u: &mut Unstructured,
    ) -> Result<WasiValue, arbitrary::Error> {
        Ok(match self {
            | WasiType::S64 => WasiValue::S64(u.arbitrary()?),
            | WasiType::U8 => WasiValue::U8(u.arbitrary()?),
            | WasiType::U16 => WasiValue::U16(u.arbitrary()?),
            | WasiType::U32 => WasiValue::U32(u.arbitrary()?),
            | WasiType::U64 => WasiValue::U64(u.arbitrary()?),
            | WasiType::Handle => WasiValue::Handle(u.arbitrary()?),
            | WasiType::Flags(flags) => WasiValue::Flags(FlagsValue {
                fields: flags
                    .fields
                    .iter()
                    .map(|_field| u.arbitrary())
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            | WasiType::Variant(variant) => {
                let case_idx: usize = u.choose_index(variant.cases.len())?;
                let case = variant.cases.get(case_idx).unwrap();

                WasiValue::Variant(Box::new(VariantValue {
                    case_idx,
                    payload: case
                        .payload
                        .as_ref()
                        .map(|payload_type| {
                            payload_type.resolve_wasi(spec).arbitrary_value(spec, u)
                        })
                        .transpose()?,
                }))
            },
            | WasiType::Record(record) => {
                // Special case: buf and buf_len in the same record.
                if record.members.len() == 2
                    && record.members[0].name == "buf"
                    && record.members[1].name == "buf_len"
                {
                    let buf_len = u.choose_index(64)?;
                    let buf = u.bytes(buf_len)?;
                    let buf = buf.iter().map(|&b| WasiValue::U8(b)).collect_vec();

                    WasiValue::Record(RecordValue {
                        members: vec![
                            WasiValue::Pointer(PointerValue { items: buf }),
                            WasiValue::U32(buf_len as u32),
                        ],
                    })
                } else {
                    WasiValue::Record(RecordValue {
                        members: record
                            .members
                            .iter()
                            .map(|member| member.tref.resolve_wasi(spec).arbitrary_value(spec, u))
                            .collect::<Result<Vec<_>, _>>()?,
                    })
                }
            },
            | WasiType::String => {
                let n = u.choose_index(16)?;
                let mut bytes = Vec::with_capacity(n);

                for _ in 0..n {
                    bytes.push(*u.choose(&[b'.', b'/', b'a'])?);
                }

                WasiValue::String(bytes)
            },
            | WasiType::Pointer(pointer) => {
                let n = u.choose_index(16)?;
                let mut items = Vec::with_capacity(n);

                for _ in 0..n {
                    items.push(pointer.item.resolve_wasi(spec).arbitrary_value(spec, u)?);
                }

                WasiValue::Pointer(PointerValue { items })
            },
            | WasiType::List(list) => {
                let n = u.choose_index(16)?;
                let mut items = Vec::with_capacity(n);

                for _ in 0..n {
                    items.push(list.item.resolve_wasi(spec).arbitrary_value(spec, u)?);
                }

                WasiValue::List(ListValue { items })
            },
        })
    }

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

    pub fn pointer(&self) -> Option<&PointerType> {
        match self {
            | Self::Pointer(pointer) => Some(pointer),
            | _ => None,
        }
    }

    pub fn record(&self) -> Option<&RecordType> {
        match self {
            | Self::Record(record) => Some(record),
            | _ => None,
        }
    }

    fn alignment(&self, spec: &Spec) -> u32 {
        match self {
            | WasiType::U8 => 1,
            | WasiType::U16 => 2,
            | WasiType::U32 => 4,
            | WasiType::S64 | WasiType::U64 => 8,
            | WasiType::Pointer(_) => 4,
            | WasiType::List(_) => 4,
            | WasiType::Record(record) => record.alignment(spec),
            | WasiType::Variant(variant) => variant.alignment(spec),
            | WasiType::Handle => 4,
            | WasiType::Flags(flags) => flags.repr.alignment(),
            | WasiType::String => 4,
        }
    }

    fn mem_size(&self, spec: &Spec) -> u32 {
        match self {
            | Self::U8 => 1,
            | Self::U16 => 2,
            | Self::U32 => 4,
            | Self::S64 | Self::U64 => 8,
            | Self::Pointer(_) => 4,
            | Self::List(_) => 8,
            | Self::Record(record) => record.mem_size(spec),
            | Self::Variant(variant) => variant.mem_size(spec),
            | Self::Handle => 4,
            | Self::Flags(flags) => flags.repr.mem_size(),
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
    fn alignment(&self) -> u32 {
        match self {
            | IntRepr::U8 => 1,
            | IntRepr::U16 => 2,
            | IntRepr::U32 => 4,
            | IntRepr::U64 => 8,
        }
    }

    fn mem_size(&self) -> u32 {
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

    fn alignment(&self, spec: &Spec) -> u32 {
        self.tag_repr.alignment().max(self.max_case_alignment(spec))
    }

    fn mem_size(&self, spec: &Spec) -> u32 {
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

    fn payload_offset(&self, spec: &Spec) -> u32 {
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
    fn alignment(&self, spec: &Spec) -> u32 {
        self.members
            .iter()
            .map(|member| member.tref.alignment(spec))
            .max()
            .unwrap_or(1)
    }

    fn mem_size(&self, spec: &Spec) -> u32 {
        let mut size: u32 = 0;
        let alignment = self.alignment(spec);

        for member in &self.members {
            let alignment = member.tref.alignment(spec);

            size = size.div_ceil(alignment) * alignment;
            size += member.tref.mem_size(spec);
        }

        size.div_ceil(alignment) * alignment
    }

    fn member_layout(&self, spec: &Spec) -> Vec<RecordMemberLayout> {
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
pub struct PointerType {
    pub item:    TypeRef,
    pub r#const: bool,
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub enum WasiValue {
    Handle(u32),
    S64(i64),
    U8(u8),
    U32(u32),
    U16(u16),
    U64(u64),
    Record(RecordValue),
    Flags(FlagsValue),
    List(ListValue),
    Pointer(PointerValue),
    String(Vec<u8>),
    Variant(Box<VariantValue>),
}

impl WasiValue {
    pub fn u32(&self) -> Option<u32> {
        match self {
            | &WasiValue::U32(i) => Some(i),
            | _ => None,
        }
    }

    pub fn u64(&self) -> Option<u64> {
        match self {
            | &WasiValue::U64(i) => Some(i),
            | _ => None,
        }
    }

    pub fn handle(&self) -> Option<u32> {
        match self {
            | &WasiValue::Handle(handle) => Some(handle),
            | _ => None,
        }
    }

    pub fn record(&self) -> Option<&RecordValue> {
        match self {
            | WasiValue::Record(record) => Some(record),
            | _ => None,
        }
    }

    pub fn record_mut(&mut self) -> Option<&mut RecordValue> {
        match self {
            | WasiValue::Record(record) => Some(record),
            | _ => None,
        }
    }

    pub fn string(&self) -> Option<&[u8]> {
        match self {
            | WasiValue::String(b) => Some(b),
            | _ => None,
        }
    }

    pub fn variant(&self) -> Option<&VariantValue> {
        match self {
            | WasiValue::Variant(variant) => Some(variant),
            | _ => None,
        }
    }

    pub fn into_pb(self, spec: &Spec, tref: &TypeRef) -> wazzi_executor_pb_rust::Value {
        let which = match (&tref.resolve(spec).wasi, self) {
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
            | (_, Self::U16(i)) => wazzi_executor_pb_rust::value::Which::Builtin(
                wazzi_executor_pb_rust::value::Builtin {
                    which:          Some(wazzi_executor_pb_rust::value::builtin::Which::U16(i.into())),
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
            | (WasiType::Pointer(pointer), Self::Pointer(pointer_value)) => {
                let items = pointer_value.items.into_iter().map(|item| {
                    item.into_pb(spec, &pointer.item)
                }).collect();

                if pointer.r#const {
                    wazzi_executor_pb_rust::value::Which::ConstPointer(
                        wazzi_executor_pb_rust::value::Array {
                            items,
                            item_size: pointer.item.mem_size(spec),
                            special_fields: Default::default()
                        }
                    )
                } else {
                    wazzi_executor_pb_rust::value::Which::Pointer(
                        wazzi_executor_pb_rust::value::Array {
                            items,
                            item_size: pointer.item.mem_size(spec),
                            special_fields: Default::default()
                        }
                    )
                }
            },
            | (_, Self::Pointer(_pointer_value)) => unreachable!(),
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
            | (_, Self::Record(_)) | (_, Self::Flags(_)) | (_, Self::List(_)) | (_, Self::Variant(_)) => unreachable!("{:#?}", tref),
        };

        wazzi_executor_pb_rust::Value {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }

    pub fn from_pb(value: wazzi_executor_pb_rust::Value, spec: &Spec, tdef: &TypeDef) -> Self {
        match (&tdef.wasi, value.which.unwrap()) {
            | (_, wazzi_executor_pb_rust::value::Which::Handle(handle)) => Self::Handle(handle),
            | (_, wazzi_executor_pb_rust::value::Which::Builtin(builtin)) => {
                match builtin.which.unwrap() {
                    | wazzi_executor_pb_rust::value::builtin::Which::Char(_c) => panic!(),
                    | wazzi_executor_pb_rust::value::builtin::Which::U8(i) => {
                        Self::U8(i.try_into().unwrap())
                    },
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
            | (WasiType::String, wazzi_executor_pb_rust::value::Which::Array(array)) => {
                Self::String(
                    array
                        .items
                        .iter()
                        .map(|item| item.builtin().u8() as u8)
                        .collect(),
                )
            },
            | (
                WasiType::Record(record),
                wazzi_executor_pb_rust::value::Which::Record(record_value),
            ) => Self::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .zip(record_value.members)
                    .map(|(member, member_value)| {
                        WasiValue::from_pb(
                            member_value.value.unwrap(),
                            spec,
                            member.tref.resolve(spec),
                        )
                    })
                    .collect(),
            }),
            | (
                WasiType::Variant(variant_type),
                wazzi_executor_pb_rust::value::Which::Variant(variant),
            ) => {
                let case_idx = variant.case_idx as usize;
                let payload = variant_type.cases[case_idx]
                    .payload
                    .as_ref()
                    .map(|payload_tref| {
                        let tdef = payload_tref.resolve(spec);

                        match variant.payload_option {
                            | Some(
                                wazzi_executor_pb_rust::value::variant::Payload_option::PayloadSome(
                                    p,
                                ),
                            ) => Self::from_pb(*p, spec, tdef),
                            | _ => panic!(),
                        }
                    });

                Self::Variant(Box::new(VariantValue { case_idx, payload }))
            },
            | _ => unreachable!("{:#?}", tdef),
        }
    }
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct RecordValue {
    pub members: Vec<WasiValue>,
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct FlagsValue {
    pub fields: Vec<bool>,
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct PointerValue {
    pub items: Vec<WasiValue>,
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct ListValue {
    pub items: Vec<WasiValue>,
}

#[derive(Serialize, Deserialize, Hash, PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct VariantValue {
    pub case_idx: usize,
    pub payload:  Option<WasiValue>,
}

fn align_to(ptr: u32, alignment: u32) -> u32 {
    ptr.div_ceil(alignment) * alignment
}
