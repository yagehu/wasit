use serde::{Deserialize, Serialize};

use crate::action::{BuiltinValue, Value};

pub mod store;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WasiSnapshot {
    pub errno:       Option<i32>,
    pub param_views: Vec<ValueView>,
    pub params:      Vec<Value>,
    pub results:     Vec<ValueView>,

    #[serde(skip)]
    pub linear_memory: Vec<u8>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ValueView {
    pub memory_offset: u32,
    pub value:         PureValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PureValue {
    Builtin(BuiltinValue),
    Handle(u32),
    List(Vec<ValueView>),
    Record(Vec<PureRecordMemeber>),
    Pointer(Vec<ValueView>),
    Variant(PureVariant),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PureRecordMemeber {
    pub name: String,
    pub view: ValueView,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PureVariant {
    pub case_idx:  u32,
    pub case_name: String,
    pub payload:   Option<Box<ValueView>>,
}
