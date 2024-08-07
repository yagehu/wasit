use std::collections::HashMap;

use crate::preview1::spec::WasiValue;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Context {
    pub resources: HashMap<usize, (WasiValue, Option<Vec<u8>>)>,
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
