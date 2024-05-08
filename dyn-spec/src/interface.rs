use std::collections::HashMap;

use crate::{ast::Idx, wasi::Type};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Interface {
    pub(crate) types:    Vec<Type>,
    pub(crate) type_map: HashMap<String, usize>,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            types:    Default::default(),
            type_map: Default::default(),
        }
    }

    pub fn get_type(&self, idx: &Idx) -> Option<&Type> {
        let i = match idx {
            | Idx::Symbolic(name) => *self.type_map.get(name)?,
            | &Idx::Numeric(i) => i,
        };

        self.types.get(i)
    }

    pub fn push_type(&mut self, name: String, ty: Type) {
        self.types.push(ty);
        self.type_map.insert(name, self.types.len() - 1);
    }
}
