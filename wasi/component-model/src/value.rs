use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ValueMeta {
    pub value:    Value,
    pub resource: Option<ResourceMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ResourceMeta {
    pub id:   u64,
    pub name: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    S64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Handle(u32),

    // Container types
    Record(RecordValue),
    Variant(Box<VariantValue>),
    List(Vec<ValueMeta>),

    Flags(FlagsValue),
    String(StringValue),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordValue {
    pub members: Vec<ValueMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub case_idx:  u32,
    pub case_name: String,
    pub payload:   Option<ValueMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FlagsValue {
    pub members: Vec<FlagsMember>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FlagsMember {
    pub name:  String,
    pub value: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl From<StringValue> for Vec<u8> {
    fn from(value: StringValue) -> Self {
        match value {
            | StringValue::Utf8(s) => s.into_bytes(),
            | StringValue::Bytes(bytes) => bytes,
        }
    }
}

impl From<Vec<u8>> for StringValue {
    fn from(value: Vec<u8>) -> Self {
        match String::from_utf8(value) {
            | Ok(s) => Self::Utf8(s),
            | Err(err) => Self::Bytes(err.into_bytes()),
        }
    }
}
