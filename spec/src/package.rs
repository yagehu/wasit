use std::collections::BTreeMap;

use petgraph::{prelude::DiGraph, stable_graph::NodeIndex};

use crate::Error;

#[derive(Clone, Debug)]
pub struct Package {
    interfaces:      Vec<Interface>,
    interface_names: BTreeMap<String, usize>,
}

impl Package {
    pub fn new() -> Self {
        Self {
            interfaces:      Default::default(),
            interface_names: Default::default(),
        }
    }

    pub fn interface(&self, idx: TypeidxBorrow) -> Option<&Interface> {
        match idx {
            | TypeidxBorrow::Numeric(i) => self.interfaces.get(i as usize),
            | TypeidxBorrow::Symbolic(name) => {
                self.interfaces.get(*self.interface_names.get(name)?)
            },
        }
    }

    pub fn register_interface(&mut self, interface: Interface, name: Option<String>) {
        let idx = self.interfaces.len();

        self.interfaces.push(interface);

        if let Some(name) = name {
            self.interface_names.insert(name, idx);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interface {
    resources:      Vec<Resource>,
    resource_names: BTreeMap<String, usize>,
    functions:      BTreeMap<String, Function>,
    graph:          DiGraph<Node, Edge>,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            resources:      Default::default(),
            resource_names: Default::default(),
            functions:      Default::default(),
            graph:          Default::default(),
        }
    }

    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }

    pub fn resolve_valtype(&self, valtype: &Valtype) -> Option<Defvaltype> {
        match valtype {
            | Valtype::Typeidx(typeidx) => self.get_resource_type(typeidx.borrow()).cloned(),
            | Valtype::Defvaltype(def) => Some(def.to_owned()),
        }
    }

    pub fn register_function(&mut self, name: String, function: Function) -> Result<(), Error> {
        if self.functions.contains_key(&name) {
            return Err(Error::DuplicateName(name));
        }

        for param in &function.params {
            self.rec_validate_valtype(&param.valtype)?;
        }

        for result in &function.results {
            self.rec_validate_valtype(&result.valtype)?;
        }

        self.functions.insert(name, function);

        Ok(())
    }

    pub fn register_resource(&mut self, ty: Defvaltype, name: Option<String>) -> Result<(), Error> {
        if let Some(name) = &name {
            if self.resource_names.contains_key(name) {
                return Err(Error::DuplicateName(name.to_owned()));
            }
        }

        let idx = self.resources.len();
        let node_idx = self.graph.add_node(Node::Resource { def: ty.clone() });

        self.resources.push(Resource { node_idx, def: ty });

        if let Some(name) = name {
            self.resource_names.insert(name, idx);
        }

        Ok(())
    }

    pub fn register_resource_relation(
        &mut self,
        resource_idx: TypeidxBorrow,
        fulfills: TypeidxBorrow,
    ) -> Result<(), Error> {
        let resource = self
            .get_resource(resource_idx.clone())
            .ok_or_else(|| Error::InvalidTypeidx(resource_idx.to_owned()))?
            .node_idx;
        let target_resource = self
            .get_resource(fulfills.clone())
            .ok_or_else(|| Error::InvalidTypeidx(fulfills.to_owned()))?
            .node_idx;

        self.graph
            .add_edge(resource, target_resource, Edge::Fulfills);
        self.graph
            .add_edge(resource, target_resource, Edge::Fulfills);

        Ok(())
    }

    pub fn get_resource_type(&self, idx: TypeidxBorrow) -> Option<&Defvaltype> {
        self.get_resource(idx).map(|resource| &resource.def)
    }

    fn get_resource(&self, idx: TypeidxBorrow) -> Option<&Resource> {
        match idx {
            | TypeidxBorrow::Numeric(i) => self.resources.get(i as usize),
            | TypeidxBorrow::Symbolic(id) => self.resources.get(*self.resource_names.get(id)?),
        }
    }

    fn rec_validate_valtype(&self, valtype: &Valtype) -> Result<(), Error> {
        match valtype {
            | Valtype::Typeidx(typeidx) => match typeidx {
                | &Typeidx::Numeric(idx) => {
                    if idx as usize >= self.resources.len() {
                        return Err(Error::InvalidTypeidx(typeidx.to_owned()));
                    }
                },
                | Typeidx::Symbolic(name) => {
                    if !self.resource_names.contains_key(name) {
                        return Err(Error::InvalidTypeidx(typeidx.to_owned()));
                    }
                },
            },
            | Valtype::Defvaltype(defvaltype) => match defvaltype {
                | Defvaltype::U8 => (),
                | Defvaltype::U32 => (),
                | Defvaltype::U64 => (),
                | Defvaltype::List(list) => self.rec_validate_valtype(&list.element)?,
                | Defvaltype::Record(_record) => todo!(),
                | Defvaltype::Variant(variant) => {
                    for case in &variant.cases {
                        if let Some(valtype) = &case.payload {
                            self.rec_validate_valtype(valtype)?;
                        }
                    }
                },
                | Defvaltype::Handle => (),
                | Defvaltype::Flags(_) => (),
                | Defvaltype::Tuple(tuple) => {
                    for valtype in tuple {
                        self.rec_validate_valtype(valtype)?;
                    }
                },
                | Defvaltype::Result(result) => {
                    if let Some(ok) = &result.ok {
                        self.rec_validate_valtype(ok)?;
                    }

                    self.rec_validate_valtype(&result.error)?;
                },
                | Defvaltype::String => (),
            },
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub name:    String,
    pub params:  Vec<FunctionParam>,
    pub results: Vec<FunctionParam>,
}

impl Function {
    pub fn unpack_expected_result(&self) -> Vec<Valtype> {
        let mut v = Vec::new();

        if let Some(result) = self.results.first() {
            match &result.valtype {
                | Valtype::Typeidx(_typeidx) => panic!("result cannot be a name"),
                | Valtype::Defvaltype(def) => match def {
                    | Defvaltype::Result(result) => {
                        if let Some(ok) = &result.ok {
                            match ok {
                                | Valtype::Typeidx(_) => v.push(ok.clone()),
                                | Valtype::Defvaltype(def) => match def {
                                    | Defvaltype::Tuple(members) => v.extend(members.clone()),
                                    | _ => v.push(ok.clone()),
                                },
                            }
                        }
                    },
                    | _ => panic!("result must be a variant"),
                },
            }
        }

        v
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name:    String,
    pub valtype: Valtype,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum TypeidxBorrow<'a> {
    Numeric(u32),
    Symbolic(&'a str),
}

impl<'a> TypeidxBorrow<'a> {
    fn to_owned(&self) -> Typeidx {
        match self {
            | &TypeidxBorrow::Numeric(i) => Typeidx::Numeric(i),
            | &TypeidxBorrow::Symbolic(s) => Typeidx::Symbolic(s.to_owned()),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Typeidx {
    Numeric(u32),
    Symbolic(String),
}

impl Typeidx {
    fn borrow(&self) -> TypeidxBorrow {
        match self {
            | &Typeidx::Numeric(i) => TypeidxBorrow::Numeric(i),
            | Typeidx::Symbolic(name) => TypeidxBorrow::Symbolic(name),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Valtype {
    Typeidx(Typeidx),
    Defvaltype(Defvaltype),
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Resource {
    node_idx: NodeIndex,
    def:      Defvaltype,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Defvaltype {
    // Fundamental numerical value types
    U8,
    U32,
    U64,

    // Fundamental container value types
    List(Box<ListType>),
    Record(Record),
    Variant(Variant),

    Handle,

    Flags(FlagsType),
    Tuple(Vec<Valtype>),
    Result(Box<ResultType>),
    String,
}

impl Defvaltype {
    pub fn mem_size(&self) -> u32 {
        match self {
            | Defvaltype::U8 => 1,
            | Defvaltype::U32 => 4,
            | Defvaltype::U64 => 8,
            | Defvaltype::List(_) => todo!(),
            | Defvaltype::Record(_) => todo!(),
            | Defvaltype::Variant(_) => todo!(),
            | Defvaltype::Handle => 4,
            | Defvaltype::Flags(_) => todo!(),
            | Defvaltype::Tuple(_) => todo!(),
            | Defvaltype::Result(_) => todo!(),
            | Defvaltype::String => todo!(),
        }
    }

    pub fn alignment(&self, interface: &Interface) -> u32 {
        match self {
            | Defvaltype::U8 => 1,
            | Defvaltype::U32 => 4,
            | Defvaltype::U64 => 8,
            | Defvaltype::List(_) => todo!(),
            | Defvaltype::Record(record) => record.alignment(interface),
            | Defvaltype::Variant(_) => todo!(),
            | Defvaltype::Handle => 4,
            | Defvaltype::Flags(_) => todo!(),
            | Defvaltype::Tuple(_) => todo!(),
            | Defvaltype::Result(_) => todo!(),
            | Defvaltype::String => todo!(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {
    pub element: Valtype,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Record {
    pub members: Vec<RecordMember>,
}

impl Record {
    pub fn alignment(&self, interface: &Interface) -> u32 {
        self.members
            .iter()
            .map(|member| {
                interface
                    .resolve_valtype(&member.ty)
                    .unwrap()
                    .alignment(interface)
            })
            .max()
            .unwrap_or(1)
    }

    pub fn member_layout(&self, interface: &Interface) -> Vec<RecordMemberLayout> {
        let mut offset: u32 = 0;
        let mut layout = Vec::with_capacity(self.members.len());

        for member in &self.members {
            let def = interface.resolve_valtype(&member.ty).unwrap();
            let alignment = def.alignment(interface);

            offset = offset.div_ceil(alignment) * alignment;
            layout.push(RecordMemberLayout { offset });
            offset += def.mem_size();
        }

        layout
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMember {
    pub name: String,
    pub ty:   Valtype,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberLayout {
    pub offset: u32,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variant {
    pub tag_repr: Repr,
    pub cases:    Vec<VariantCase>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantCase {
    pub name:    String,
    pub payload: Option<Valtype>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType {
    pub repr:    Repr,
    pub members: Vec<String>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ResultType {
    pub ok:    Option<Valtype>,
    pub error: Valtype,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Repr {
    U16,
    U32,
    U64,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Node {
    Resource { def: Defvaltype },
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Edge {
    Fulfills,
}
