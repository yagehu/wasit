extern crate wazzi_executor_pb_rust as executor_pb;
extern crate wazzi_witx as witx;

pub mod prog;

pub(crate) mod resource_ctx;

mod snapshot;

pub use prog::{FinalProg, Prog, Value};
pub use snapshot::{FsSnapshotStore, InMemorySnapshotStore, SnapshotStore, WasiSnapshot};
