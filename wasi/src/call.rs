use arbitrary::Unstructured;
use serde::{Deserialize, Serialize};
use wazzi_witx::InterfaceFunc;

use crate::prog::Prog;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func:    String,
    pub params:  Vec<CallParam>,
    pub results: Vec<CallResult>,
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

        Ok(Self {
            func:    func_spec.name.as_str().to_owned(),
            params:  vec![],
            results: vec![],
        })
    }

    pub fn arbitrary_from_func_spec(u: &mut Unstructured, prog: &Prog, spec: &InterfaceFunc) {
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CallResult {
    Resource(u64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CallParam {
    Resource(u64),
    Value(Value),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    String(StringValue),
    Bitflags(BitflagsValue),
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
