use arbitrary::Unstructured;
use serde::{Deserialize, Serialize};
use wazzi_witx::InterfaceFunc;

use crate::prog::Prog;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func: String,

    #[serde(default)]
    pub params: Vec<Value>,

    #[serde(default)]
    pub results: Vec<CallResultSpec>,
}

impl Call {
    pub fn arbitrary(
        u: &mut Unstructured,
        prog: &Prog,
        spec: &witx::Document,
    ) -> Result<Self, arbitrary::Error> {
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();
        let func_specs = module_spec.funcs().collect::<Vec<_>>();
        let func_spec = u.choose(&func_specs)?;

        Self::arbitrary_from_func_spec(u, prog, func_spec)
    }

    pub fn arbitrary_from_func_spec(
        _u: &mut Unstructured,
        _prog: &Prog,
        spec: &InterfaceFunc,
    ) -> Result<Self, arbitrary::Error> {
        Ok(Self {
            func:    spec.name.as_str().to_owned(),
            params:  vec![],
            results: vec![],
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CallResultSpec {
    Ignore,
    Resource(u64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Resource(u64),
    RawValue(RawValue),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RawValue {
    Builtin(BuiltinValue),
    String(StringValue),
    Bitflags(BitflagsValue),
    Handle(u32),
    Array(ArrayValue),
    Record(RecordValue),
    ConstPointer(ConstPointerValue),
    Pointer(PointerValue),
    Variant(VariantValue),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinValue {
    U8(u8),
    U32(u32),
    U64(u64),
    S64(i64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BitflagsValue {
    pub repr:    BitflagsRepr,
    pub members: Vec<BitflagsMember>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BitflagsMember {
    pub name:  String,
    pub value: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BitflagsRepr {
    U8,
    U16,
    U32,
    U64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct ArrayValue(pub Vec<Value>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct RecordValue(pub Vec<RecordMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordMemberValue {
    pub name:  String,
    pub value: Value,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct ConstPointerValue(pub Vec<RawValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PointerValue {
    Alloc(PointerAlloc),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PointerAlloc {
    Resource(u64),
    Value(u32),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name:    String,
    pub payload: Option<Box<Value>>,
}
