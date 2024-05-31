pub mod effects;
pub mod term;

mod value;

use arbitrary::Unstructured;
pub use term::Term;
pub use value::*;

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Spec {
    pub types:          Vec<WazziType>,
    pub interfaces:     Vec<Interface>,
    pub types_map:      HashMap<String, usize>,
    pub interfaces_map: HashMap<String, usize>,
}

impl Spec {
    pub fn new() -> Self {
        Self {
            types:          Default::default(),
            interfaces:     Default::default(),
            types_map:      Default::default(),
            interfaces_map: Default::default(),
        }
    }
}

impl Default for Spec {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct WazziType {
    pub name:       Option<String>,
    pub wasi:       WasiType,
    pub attributes: Vec<(String, WazziType)>,
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

    pub fn zero_value(&self) -> WasiValue {
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
                    .map(|payload| payload.wasi.zero_value()),
            })),
            | WasiType::Record(_) => todo!(),
            | WasiType::String => WasiValue::String(Vec::new()),
            | WasiType::List(_) => todo!(),
        }
    }

    pub fn arbitrary_value(&self, u: &mut Unstructured) -> Result<WasiValue, arbitrary::Error> {
        Ok(match self {
            | WasiType::S64 => WasiValue::S64(u.arbitrary()?),
            | WasiType::U8 => todo!(),
            | WasiType::U16 => todo!(),
            | WasiType::U32 => todo!(),
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
                        .map(|t| t.wasi.arbitrary_value(u))
                        .transpose()?,
                }))
            },
            | WasiType::Record(_) => todo!(),
            | WasiType::String => WasiValue::String(u.arbitrary()?),
            | WasiType::List(_) => todo!(),
        })
    }

    pub fn mem_size(&self) -> u32 {
        match self {
            | Self::U8 => 1,
            | Self::U16 => 2,
            | Self::U32 => 4,
            | Self::S64 | Self::U64 => 8,
            | Self::List(_) => 8,
            | Self::Record(record) => record.mem_size(),
            | Self::Variant(variant) => variant.mem_size(),
            | Self::Handle => 4,
            | Self::Flags(flags) => flags.repr.mem_size(),
            | Self::String => todo!(),
        }
    }

    pub fn alignment(&self) -> u32 {
        match self {
            | Self::U8 => 1,
            | Self::U16 => 2,
            | Self::U32 => 4,
            | Self::S64 | Self::U64 => 8,
            | Self::List(_) => 4,
            | Self::Record(record) => record.alignment(),
            | Self::Variant(variant) => variant.alignment(),
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

    pub fn alignment(&self) -> u32 {
        self.tag_repr.alignment().max(self.max_case_alignment())
    }

    pub fn mem_size(&self) -> u32 {
        let mut size = self.tag_repr.mem_size();

        size = align_to(size, self.max_case_alignment());
        size += self
            .cases
            .iter()
            .filter_map(|case| case.payload.as_ref())
            .map(|payload| payload.wasi.mem_size())
            .max()
            .unwrap_or(0);

        align_to(size, self.alignment())
    }

    pub fn payload_offset(&self) -> u32 {
        let size = self.tag_repr.mem_size();

        align_to(size, self.max_case_alignment())
    }

    fn max_case_alignment(&self) -> u32 {
        self.cases
            .iter()
            .filter_map(|case| case.payload.as_ref())
            .map(|payload| payload.wasi.alignment())
            .max()
            .unwrap_or(1)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantCaseType {
    pub name:    String,
    pub payload: Option<WazziType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordType {
    pub members: Vec<RecordMemberType>,
}

impl RecordType {
    pub fn mem_size(&self) -> u32 {
        let mut size: u32 = 0;
        let alignment = self.alignment();

        for member in &self.members {
            let alignment = member.ty.wasi.alignment();

            size = size.div_ceil(alignment) * alignment;
            size += member.ty.wasi.mem_size();
        }

        size.div_ceil(alignment) * alignment
    }

    pub fn alignment(&self) -> u32 {
        self.members
            .iter()
            .map(|member| member.ty.wasi.alignment())
            .max()
            .unwrap_or(1)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberType {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {
    pub item: WazziType,
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
    pub input_contract: Option<Term>,
    pub effects:        effects::Program,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionResult {
    pub name: String,
    pub ty:   WazziType,
}

fn align_to(ptr: u32, alignment: u32) -> u32 {
    ptr.div_ceil(alignment) * alignment
}
