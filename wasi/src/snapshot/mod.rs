use serde::{Deserialize, Serialize};

use crate::call::{RawValue, Value};

pub mod store;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WasiSnapshot {
    pub errno:   Option<i32>,
    pub params:  Vec<Value>,
    pub results: Vec<CallResult>,

    #[serde(skip)]
    pub linear_memory: Vec<u8>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallResult {
    pub memory_offset: u32,
    pub value:         RawValue,
}
