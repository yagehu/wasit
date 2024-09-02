pub mod spec;

mod strategy;

use idxspace::IndexSpace;
pub use strategy::{CallStrategy, StatelessStrategy};

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use spec::WasiValue;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    resources:          Vec<Resource>,
    resources_by_types: HashMap<String, BTreeSet<usize>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            resources:          Default::default(),
            resources_by_types: Default::default(),
        }
    }

    pub fn new_resource(&mut self, r#type: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(r#type)
            .or_default()
            .insert(self.resources.len() - 1);

        self.resources.len() - 1
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub attributes: BTreeMap<String, WasiValue>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ValueMeta {
    pub wasi:     WasiValue,
    pub resource: Option<usize>,
}
