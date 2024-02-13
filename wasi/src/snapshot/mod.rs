mod store;

pub use store::{FsSnapshotStore, InMemorySnapshotStore, SnapshotStore};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WasiSnapshot {
    pub errno: Option<i32>,
    // pub params:  Vec<Value>,
    // pub results: Vec<Value>,
}
