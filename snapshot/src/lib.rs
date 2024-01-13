use serde::{Deserialize, Serialize};

pub mod store;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WasiSnapshot {
    pub errno: Option<i32>,

    #[serde(skip)]
    pub linear_memory: Vec<u8>,
}
