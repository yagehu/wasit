use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub mount_base_dir: bool,
    pub actions:        Vec<Action>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Decl(Decl),
    Call(Call),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Decl {
    pub resource_id:   u64,
    pub resource_type: String,
    pub value:         Value,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func: String,

    #[serde(default)]
    pub params: Vec<ResourceOrValue>,

    #[serde(default)]
    pub results: Vec<ResultSpec>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ResultSpec {
    Resource(u64),
    Ignore,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOrValue {
    Resource(u64),
    Value(Value),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Builtin(BuiltinValue),
    String(StringValue),
    Bitflags(BitflagsValue),
    Record(RecordValue),
    List(ListValue),
    ConstPointer(ListValue),
    Pointer(PointerValue),
    Variant(VariantValue),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinValue {
    U8(u8),
    U32(u32),
    U64(u64),
    S64(i64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl From<Vec<u8>> for StringValue {
    fn from(x: Vec<u8>) -> Self {
        match String::from_utf8(x) {
            | Ok(s) => Self::Utf8(s),
            | Err(err) => Self::Bytes(err.into_bytes()),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsValue(pub Vec<BitflagsMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsMemberValue {
    pub name:  String,
    pub value: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordValue(pub Vec<RecordMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordMemberValue {
    pub name:  String,
    pub value: ResourceOrValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ListValue(pub Vec<ResourceOrValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PointerValue {
    pub alloc_from_resource: u64,
    pub default_value:       Option<Box<Value>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name:    String,
    pub payload: Option<Box<ResourceOrValue>>,
}
