use serde::{Deserialize, Serialize};

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
pub enum Value {}
