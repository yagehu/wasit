use serde::{Deserialize, Serialize};

use crate::spec::WasiValue;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub state: WasiValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Resources(Vec<Resource>);

impl Resources {
    pub fn new() -> Self {
        Self(Default::default())
    }

    pub fn push(&mut self, resource: Resource) -> ResourceIdx {
        self.0.push(resource);

        ResourceIdx(self.0.len() - 1)
    }

    pub fn get(&self, i: ResourceIdx) -> Option<&Resource> {
        self.0.get(i.0)
    }

    pub fn get_mut(&mut self, i: ResourceIdx) -> Option<&mut Resource> {
        self.0.get_mut(i.0)
    }
}

impl Default for Resources {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Debug)]
pub struct ResourceIdx(pub(crate) usize);

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum HighLevelValue {
    Resource(ResourceIdx),
    Concrete(WasiValue),
}
