use std::collections::{BTreeMap, BTreeSet, HashMap};

use arbitrary::Unstructured;
use eyre::ContextCompat;
use serde::{Deserialize, Serialize};
use wazzi_spec::package::{Defvaltype, Interface, Typeidx, TypeidxBorrow, Valtype};
use wazzi_wasi_component_model::value::{
    FlagsMember,
    FlagsValue,
    RecordValue,
    StringValue,
    Value,
    VariantValue,
};

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

    // pub fn iter_by_type(&self) -> impl Iterator<Item = (&String, &BTreeSet<ResourceId>)> {
    //     self.by_types.iter()
    // }

    fn arbitrary_value(
        &self,
        u: &mut Unstructured,
        interface: &Interface,
        def: &Defvaltype,
    ) -> Result<Value, eyre::Error> {
        Ok(match def {
            | Defvaltype::S64 => Value::S64(u.arbitrary()?),
            | Defvaltype::U8 => Value::U8(u.arbitrary()?),
            | Defvaltype::U32 => Value::U32(u.arbitrary()?),
            | Defvaltype::U64 => Value::U64(u.arbitrary()?),
            | Defvaltype::List(list) => {
                let len = u.int_in_range(0..=3)? as usize;
                let mut items = Vec::with_capacity(len);

                for _i in 0..len {
                    items.push(self.arbitrary_value_from_valtype(u, interface, &list.element)?);
                }

                Value::List(items)
            },
            | Defvaltype::Record(record) => Value::Record(RecordValue {
                members: record
                    .members
                    .iter()
                    .map(|member| self.arbitrary_value_from_valtype(u, interface, &member.ty))
                    .collect::<Result<_, _>>()?,
            }),
            | Defvaltype::Variant(variant) => {
                let case_idx = u.int_in_range(0..=(variant.cases.len() - 1))?;
                let case = variant.cases.get(case_idx).unwrap();

                Value::Variant(Box::new(VariantValue {
                    case_idx:  case_idx as u32,
                    case_name: case.name.clone(),
                    payload:   case
                        .payload
                        .as_ref()
                        .map(|payload| self.arbitrary_value_from_valtype(u, interface, payload))
                        .transpose()?,
                }))
            },
            | Defvaltype::Handle => Value::Handle(u.arbitrary()?),
            | Defvaltype::Flags(flags) => Value::Flags(FlagsValue {
                members: flags
                    .members
                    .iter()
                    .map(|member| -> Result<_, arbitrary::Error> {
                        Ok(FlagsMember {
                            name:  member.clone(),
                            value: u.arbitrary()?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            }),
            | Defvaltype::Tuple(_) => todo!(),
            | Defvaltype::Result(_) => todo!(),
            | Defvaltype::String => {
                let len = u.int_in_range(0..=3)? as usize;
                let mut bytes = vec![0; len];

                u.fill_buffer(&mut bytes)?;

                Value::String(StringValue::Bytes(bytes))
            },
        })
    }

    fn arbitrary_resource_value(
        &self,
        u: &mut Unstructured,
        interface: &Interface,
        resource_name: &str,
    ) -> Result<Value, eyre::Error> {
        let resource = interface
            .resource_by_name(TypeidxBorrow::Symbolic(resource_name))
            .wrap_err("resource not found in spec")?;

        match self.by_types.get(resource_name) {
            | Some(pool) => {
                let pool = pool.iter().copied().collect::<Vec<_>>();
                let randomly_generate = u.ratio(9, 10)?;

                if randomly_generate {
                    return self.arbitrary_value(u, interface, &resource.def);
                }

                let resource_id = *u.choose(&pool)?;

                Ok(self.map.get(&resource_id).unwrap().clone())
            },
            | None => self.arbitrary_value(u, interface, &resource.def),
        }
    }

    pub fn arbitrary_value_from_valtype(
        &self,
        u: &mut Unstructured,
        interface: &Interface,
        valtype: &Valtype,
    ) -> Result<Value, eyre::Error> {
        match valtype {
            | Valtype::Typeidx(typeidx) => match typeidx {
                | Typeidx::Numeric(_) => todo!(),
                | Typeidx::Symbolic(name) => self.arbitrary_resource_value(u, interface, name),
            },
            | Valtype::Defvaltype(def) => self.arbitrary_value(u, interface, def),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id:     u64,
    pub r#type: ResourceType,
    pub value:  Value,
}
