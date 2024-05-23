pub mod term;

mod value;

pub use term::Term;
pub use value::*;

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Spec {
    pub types:      HashMap<String, WazziType>,
    pub interfaces: HashMap<String, Interface>,
}

impl Spec {
    pub fn new() -> Self {
        Self {
            types:      Default::default(),
            interfaces: Default::default(),
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
    pub name: Option<String>,
    pub wasi: WasiType,
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
                .find(|(_, case)| &case.name == case_name)
                .map(|(i, _)| i)?,
            payload,
        })))
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
