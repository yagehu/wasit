use std::collections::{BTreeSet, HashMap};

use crate::{ast::Idx, wasi, IndexSpace};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub value: wasi::Value,
    pub attrs: HashMap<String, wasi::Value>,
}

pub struct ResourceContext {
    resources: IndexSpace<Resource>,
    by_types:  HashMap<usize, BTreeSet<usize>>,
}

impl ResourceContext {
    pub fn new() -> Self {
        Self {
            resources: Default::default(),
            by_types:  Default::default(),
        }
    }

    pub fn push(&mut self, resource_type_idx: usize, resource: Resource) -> usize {
        let idx = self.resources.push(None, resource);

        self.by_types
            .entry(resource_type_idx)
            .or_default()
            .insert(idx);

        idx
    }

    pub fn get(&self, idx: &Idx) -> Option<&Resource> {
        self.resources.get(idx)
    }

    pub fn get_by_type(&self, type_idx: usize) -> Vec<(usize, &Resource)> {
        self.by_types
            .get(&type_idx)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|&i| (i, self.resources.get(&Idx::Numeric(i)).unwrap()))
            .collect()
    }
}
