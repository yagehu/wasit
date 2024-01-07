pub mod wazzi_executor_capnp {
    include!(concat!(env!("OUT_DIR"), "/wazzi_executor_capnp.rs"));
}

pub use wazzi_executor_capnp::*;
