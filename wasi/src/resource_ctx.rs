use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::prog::Value;

type ResourceId = u64;
type ResourceType = String;

#[derive(Debug, Clone)]
pub struct ResourceContext {
    next_id:  ResourceId,
    map:      HashMap<ResourceId, Value>,
    by_types: BTreeMap<ResourceType, BTreeSet<ResourceId>>,
}

impl Default for ResourceContext {
    fn default() -> Self {
        Self::new()
    }
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

    pub fn get_resource(&self, id: ResourceId) -> Option<Resource> {
        self.map.get(&id).cloned().map(|value| {
            for (ty, resources) in &self.by_types {
                if resources.contains(&id) {
                    return Resource {
                        id,
                        r#type: ty.to_owned(),
                        value,
                    };
                }
            }

            unreachable!()
        })
    }

    pub fn iter_by_type(&self) -> impl Iterator<Item = (&String, &BTreeSet<ResourceId>)> {
        self.by_types.iter()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id:     u64,
    pub r#type: ResourceType,
    pub value:  Value,
}
