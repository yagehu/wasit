use arbitrary::Unstructured;
use wazzi_spec::parsers::wazzi_preview1;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TopLevelType {
    Unit,
    Bool,
    I64,
    U32,
    U64,
    Flags,
    Variant,
    String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Type {
    Unit,
    Bool,
    S64,
    U32,
    U64,
    Handle,
    Flags(FlagsType),
    Variant(VariantType),
    String,
}

impl Type {
    pub fn variant(&self) -> Option<&VariantType> {
        match self {
            | Self::Variant(x) => Some(x),
            | _ => None,
        }
    }

    pub fn from_preview1_type(ty: &wazzi_preview1::Type) -> Self {
        match ty {
            | wazzi_preview1::Type::S64(_) => Self::S64,
            | wazzi_preview1::Type::U8(_) => todo!(),
            | wazzi_preview1::Type::U16(_) => todo!(),
            | wazzi_preview1::Type::U32(_) => Self::U32,
            | wazzi_preview1::Type::U64(_) => Self::U64,
            | wazzi_preview1::Type::Record(_) => todo!(),
            | wazzi_preview1::Type::Enum(e) => Self::Variant(VariantType {
                cases: e
                    .cases
                    .iter()
                    .map(|case| CaseType {
                        name:    case.name().to_owned(),
                        payload: None,
                    })
                    .collect(),
            }),
            | wazzi_preview1::Type::Union(_) => todo!(),
            | wazzi_preview1::Type::List(_) => todo!(),
            | wazzi_preview1::Type::Handle(_) => Self::Handle,
            | wazzi_preview1::Type::Flags(flags) => Self::Flags(FlagsType {
                repr:   flags.repr.clone().into(),
                fields: flags
                    .members
                    .iter()
                    .map(|id| id.name().to_owned())
                    .collect(),
            }),
            | wazzi_preview1::Type::Result(_) => todo!(),
            | wazzi_preview1::Type::String => Self::String,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType {
    pub repr:   IntRepr,
    pub fields: Vec<String>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantType {
    pub cases: Vec<CaseType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct CaseType {
    pub name:    String,
    pub payload: Option<Type>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    S64(i64),
    U32(i32),
    U64(u64),
    Handle(u32),
    Flags(Flags),
    Variant(Box<Variant>),
    String(Vec<u8>),
}

impl Value {
    pub fn arbitrary(ty: &Type, u: &mut Unstructured) -> Result<Self, arbitrary::Error> {
        Ok(match ty {
            | Type::Unit => Self::Unit,
            | Type::Bool => Self::Bool(u.arbitrary()?),
            | Type::S64 => Self::S64(u.arbitrary()?),
            | Type::U32 => Self::U32(u.arbitrary()?),
            | Type::U64 => Self::U64(u.arbitrary()?),
            | Type::Handle => Self::Handle(u.arbitrary()?),
            | Type::Flags(flags) => Self::Flags(Flags {
                repr:   flags.repr,
                fields: flags
                    .fields
                    .iter()
                    .map(|_| u.arbitrary::<bool>())
                    .collect::<Result<_, _>>()?,
            }),
            | Type::Variant(variant) => {
                let cases = variant.cases.iter().enumerate().collect::<Vec<_>>();
                let &(case_idx, _case) = u.choose(&cases)?;

                Self::Variant(Box::new(Variant {
                    case_idx,
                    payload: None,
                }))
            },
            | Type::String => Self::String(u.arbitrary()?),
        })
    }

    pub fn bool_(&self) -> Option<bool> {
        match self {
            | &Value::Bool(b) => Some(b),
            | _ => None,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Flags {
    pub repr:   IntRepr,
    pub fields: Vec<bool>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variant {
    pub case_idx: usize,
    pub payload:  Option<Value>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum IntRepr {
    U8,
    U16,
    U32,
    U64,
}

impl From<wazzi_preview1::Repr<'_>> for IntRepr {
    fn from(value: wazzi_preview1::Repr) -> Self {
        match value {
            | wazzi_preview1::Repr::U8(_) => Self::U8,
            | wazzi_preview1::Repr::U16(_) => Self::U16,
            | wazzi_preview1::Repr::U32(_) => Self::U32,
            | wazzi_preview1::Repr::U64(_) => Self::U64,
        }
    }
}
