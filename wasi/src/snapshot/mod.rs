use serde::{Deserialize, Serialize};

use crate::call::{BuiltinValue, RawValue, Value};

pub mod store;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WasiSnapshot {
    pub errno:       Option<i32>,
    pub param_views: Vec<ValueView>,
    pub params:      Vec<Value>,
    pub results:     Vec<CallResult>,

    #[serde(skip)]
    pub linear_memory: Vec<u8>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ValueView {
    pub memory_offset: usize,
    pub memory_len:    usize,
    pub value:         PureValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PureValue {
    Builtin(BuiltinValue),
    Handle(u32),
    List(Vec<ValueView>),
    Record(Vec<PureRecordMemeberValue>),
    Pointer(Vec<ValueView>),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PureRecordMemeberValue {
    pub name:  String,
    pub value: PureValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallResult {
    pub memory_offset: u32,
    pub value:         RawValue,
}
