use std::collections::{BTreeSet, HashMap};

use crate::prog::stateful::Value;

pub(crate) type ResourceId = u64;
pub(crate) type ResourceType = String;

#[derive(Debug, Clone)]
pub struct ResourceContext {
    next_id:  ResourceId,
    map:      HashMap<ResourceId, Value>,
    by_types: HashMap<ResourceType, BTreeSet<ResourceId>>,
}

impl ResourceContext {
    pub fn new() -> Self {
        Self {
            next_id:  0,
            map:      Default::default(),
            by_types: Default::default(),
        }
    }

    pub fn new_resource(&mut self, ty: &str, value: Value) {
        self.register_resource(ty, value, self.next_id);
    }

    pub fn register_resource(&mut self, ty: &str, value: Value, id: ResourceId) {
        let resources = match self.by_types.get_mut(ty) {
            | Some(set) => set,
            | None => {
                self.by_types.insert(ty.to_owned(), Default::default());
                self.by_types.get_mut(ty).unwrap()
            },
        };

        self.next_id = id + 1;
        self.map.insert(id, value);
        resources.insert(id);
    }

    pub fn get_resource(&self, id: ResourceId) -> Option<Value> {
        self.map.get(&id).cloned()
    }
}
