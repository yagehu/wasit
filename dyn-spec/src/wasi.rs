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
    I64,
    U32,
    U64,
    Variant(VariantType),
}

impl Type {
    pub fn variant(&self) -> Option<&VariantType> {
        match self {
            | Self::Variant(x) => Some(x),
            | _ => None,
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
