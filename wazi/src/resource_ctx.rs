use std::collections::{BTreeMap, BTreeSet, HashMap};

use eyre::ContextCompat;
use serde::{Deserialize, Serialize};
use wazzi_spec::package::{Defvaltype, Interface, Typeidx, Valtype};
use wazzi_wasi_component_model::value::Value;

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

    pub fn register_resource_rec(
        &mut self,
        interface: &Interface,
        valtype: &Valtype,
        value: Value,
        resource_id: Option<u64>,
    ) -> Result<(), eyre::Error> {
        match valtype {
            | Valtype::Typeidx(Typeidx::Symbolic(name)) => match resource_id {
                | Some(resource_id) => self.register_resource(name, value.clone(), resource_id),
                | None => self.new_resource(name, value.clone()),
            },
            | _ => (),
        };

        let def = interface
            .resolve_valtype(valtype)
            .wrap_err("invalid valtype")?;

        match (def, value) {
            | (Defvaltype::S64, _)
            | (Defvaltype::U8, _)
            | (Defvaltype::U32, _)
            | (Defvaltype::U64, _) => (),
            | (Defvaltype::List(_), _) => todo!(),
            | (Defvaltype::Record(record), Value::Record(record_value)) => {
                for (member, member_value) in record.members.iter().zip(record_value.members) {
                    self.register_resource_rec(interface, &member.ty, member_value, resource_id)?;
                }
            },
            | (Defvaltype::Variant(variant), Value::Variant(variant_value)) => {
                let case = &variant.cases[variant_value.case_idx as usize];

                if let Some(payload_type) = &case.payload {
                    self.register_resource_rec(
                        interface,
                        payload_type,
                        variant_value.payload.unwrap(),
                        None,
                    )?;
                }
            },
            | (Defvaltype::Handle, _) => (),
            | (Defvaltype::Flags(_), _) => (),
            | (Defvaltype::Tuple(_), _) => todo!(),
            | (Defvaltype::Result(_), _) => todo!(),
            | (Defvaltype::String, _) => todo!(),
            | (Defvaltype::Record(_), _) | (Defvaltype::Variant(_), _) => panic!(),
        }

        Ok(())
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
