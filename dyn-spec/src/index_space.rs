use std::collections::HashMap;

use crate::ast::Idx;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IndexSpace<T> {
    stack: Vec<T>,
    map:   HashMap<String, usize>,
}

impl<T> IndexSpace<T> {
    pub fn new() -> Self {
        Self {
            stack: Default::default(),
            map:   Default::default(),
        }
    }

    pub fn push(&mut self, name: Option<String>, item: T) -> usize {
        self.stack.push(item);

        if let Some(name) = name {
            self.map.insert(name, self.stack.len() - 1);
        }

        self.stack.len() - 1
    }

    pub fn get(&self, idx: &Idx) -> Option<&T> {
        self.stack.get(self.resolve_idx(idx)?)
    }

    pub fn resolve_idx(&self, idx: &Idx) -> Option<usize> {
        match idx {
            | Idx::Symbolic(name) => self.map.get(name).cloned(),
            | &Idx::Numeric(i) => Some(i),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.stack.iter()
    }
}

impl<T> Default for IndexSpace<T> {
    fn default() -> Self {
        Self::new()
    }
}
