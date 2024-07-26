use std::collections::BTreeMap;

use idxspace::IndexSpace;
use wazzi_specz_wasi::{Spec, WasiType, WazziTypedef};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct NamedType {
    pub name: String,
    pub wasi: WasiType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TypeDefContext {
    type_defs: IndexSpace<String, TypeDef>,
}

impl TryFrom<&Spec> for TypeDefContext {
    type Error = ();

    fn try_from(spec: &Spec) -> Result<Self, Self::Error> {
        let type_defs = spec
            .types
            .iter()
            .map(|(name, t)| (name.to_string(), TypeDef {}))
            .collect::<IndexSpace<_, _>>();

        for (name, i, t) in spec.types.iter() {
            let name = spec.types_map.get_by_right(&i).unwrap();

            if t.attributes.is_empty() {
            } else {
                type_defs.push(
                    name.to_string(),
                    TypeDef {
                        name:  name.to_string(),
                        inner: TypeDefKind::Resource(Resource {
                            wasi:       t.wasi.clone(),
                            attributes: todo!(),
                        }),
                    },
                );
            }
        }

        Ok(Self { type_defs })
    }
}

#[derive(Debug)]
pub struct TypeDef<'ctx> {
    name: String,
    kind: TypeDefKind,
    z3:   z3::DatatypeSort<'ctx>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum TypeDefKind {
    Resource(Resource),
    Regular(WasiType),
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Resource {
    wasi:       WasiType,
    attributes: BTreeMap<String, NamedType>,
}

impl<'ctx> TypeDef<'ctx> {
    pub fn new(&self, ctx: &'ctx z3::Context, t: &WazziTypedef) -> Self {
        match self.tdef {
            | TypeDefKind::Resource(resource) => {
                z3::DatatypeBuilder::new(ctx, self.name.as_str()).variant(
                    &format!("{}--attrs", self.name),
                    resource
                        .attributes
                        .iter()
                        .map(|(attr, ty)| {
                            (
                                attr.as_str(),
                                z3::DatatypeAccessor::Sort(
                                    datatypes
                                        .get(ty.name.as_ref().unwrap())
                                        .unwrap()
                                        .sort
                                        .clone(),
                                ),
                            )
                        })
                        .collect(),
                );
            },
            | TypeDefKind::Regular(_) => todo!(),
        }

        Self {
            name: t.name.clone(),
            kind: (),
            z3:   (),
        }
    }
}
