use std::collections::BTreeMap;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Package {
    interfaces: BTreeMap<String, Interface>,
}

impl Package {
    pub fn new() -> Self {
        Self {
            interfaces: Default::default(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Interface {
    types: Vec<DefValType>,
}

impl Interface {
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum DefValType {
    U32,
    U64,
    List(ListType),
    Flags,
    Record,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {}
