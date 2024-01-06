pub use call::{Call, Value};
pub use prog::{Prog, ProgSeed};

extern crate wazzi_witx as witx;

mod call;
mod capnp_mappers;
mod prog;
