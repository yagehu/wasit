use serde::{Deserialize, Serialize};

use super::{seed, stateful};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FinalProg {
    pub calls: Vec<Call>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func:    String,
    pub params:  Vec<Value>,
    pub results: Vec<Value>,
    pub errno:   Option<i32>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Builtin(seed::BuiltinValue),
    Handle(u32),
    String(seed::StringValue),
}

impl From<stateful::Value> for Value {
    fn from(x: stateful::Value) -> Self {
        match x {
            | stateful::Value::Builtin(builtin) => Self::Builtin(builtin),
            | stateful::Value::Handle(handle) => Self::Handle(handle),
            | stateful::Value::String(string) => Self::String(seed::StringValue::from(string)),
            | stateful::Value::Bitflags(_) => todo!(),
        }
    }
}
