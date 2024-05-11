use wazzi_spec::parsers::wazzi_preview1;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TopLevelType {
    Unit,
    Bool,
    I64,
    U32,
    U64,
    Variant,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Type {
    Unit,
    Bool,
    S64,
    U32,
    U64,
    Handle,
    Variant(VariantType),
}

impl Type {
    pub fn variant(&self) -> Option<&VariantType> {
        match self {
            | Self::Variant(x) => Some(x),
            | _ => None,
        }
    }

    fn from_preview1_type(ty: &wazzi_preview1::Type) -> Self {
        match ty {
            | wazzi_preview1::Type::S64(_) => Self::S64,
            | wazzi_preview1::Type::U8(_) => todo!(),
            | wazzi_preview1::Type::U16(_) => todo!(),
            | wazzi_preview1::Type::U32(_) => Self::U32,
            | wazzi_preview1::Type::U64(_) => Self::U64,
            | wazzi_preview1::Type::Record(_) => todo!(),
            | wazzi_preview1::Type::Enum(_) => todo!(),
            | wazzi_preview1::Type::Union(_) => todo!(),
            | wazzi_preview1::Type::List(_) => todo!(),
            | wazzi_preview1::Type::Handle(_) => Self::Handle,
            | wazzi_preview1::Type::Flags(_) => todo!(),
            | wazzi_preview1::Type::Result(_) => todo!(),
            | wazzi_preview1::Type::String => todo!(),
        }
    }
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
    I64(i64),
    U32(i32),
    U64(u64),
    Handle(u32),
    Flags(Flags),
    Variant(Box<Variant>),
}

impl Value {
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
    pub fields: Vec<FlagField>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagField {
    pub name:  String,
    pub value: bool,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variant {
    pub case_idx:  usize,
    pub case_name: String,
    pub payload:   Option<Value>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum IntRepr {
    U8,
    U16,
    U32,
    U64,
}
