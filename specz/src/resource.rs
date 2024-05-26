use std::collections::HashMap;

use wazzi_specz_wasi::WasiValue;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Context {
    pub resources: HashMap<usize, WasiValue>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            resources: Default::default(),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
