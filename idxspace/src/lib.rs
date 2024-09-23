use std::{borrow::Borrow, hash::Hash};

use bimap::BiMap;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IndexSpace<K, V>
where
    K: PartialEq + Eq + Hash,
{
    list: Vec<V>,
    map:  BiMap<K, usize>,
}

impl<K, V> IndexSpace<K, V>
where
    K: PartialEq + Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            list: Default::default(),
            map:  Default::default(),
        }
    }

    pub fn push(&mut self, k: K, v: V) {
        self.list.push(v);
        self.map.insert(k, self.list.len() - 1);
    }

    pub fn iter(&self) -> Iter<K, V> {
        Iter::new(self)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn get_by_key<Q>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.list.get(*self.map.get_by_left(k)?)
    }
}

impl<K, V> Default for IndexSpace<K, V>
where
    K: PartialEq + Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FromIterator<(K, V)> for IndexSpace<K, V>
where
    K: PartialEq + Eq + Hash,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut idxspace = Self::default();

        for (k, v) in iter.into_iter() {
            idxspace.push(k, v);
        }

        idxspace
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Iter<'s, K, V>
where
    K: PartialEq + Eq + Hash,
{
    idx_space: &'s IndexSpace<K, V>,
    next:      usize,
}

impl<'s, K, V> Iter<'s, K, V>
where
    K: PartialEq + Eq + Hash,
{
    fn new(idx_space: &'s IndexSpace<K, V>) -> Self {
        Self { idx_space, next: 0 }
    }
}

impl<'s, K, V> Iterator for Iter<'s, K, V>
where
    K: PartialEq + Eq + Hash,
{
    type Item = (&'s K, &'s V);

    fn next(&mut self) -> Option<Self::Item> {
        let v = self.idx_space.list.get(self.next)?;
        let k = self.idx_space.map.get_by_right(&self.next)?;

        self.next += 1;

        Some((k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order() {
        let mut s = IndexSpace::new();

        s.push("a".to_string(), 0i32);
        s.push("b".to_string(), 1);
        s.push("c".to_string(), 2);

        let mut iter = s.iter();
        let t0 = iter.next().unwrap();
        let t1 = iter.next().unwrap();
        let t2 = iter.next().unwrap();

        assert_eq!(t0.0, "a");
        assert_eq!(t0.1, &0);

        assert_eq!(t1.0, "b");
        assert_eq!(t1.1, &1);

        assert_eq!(t2.0, "c");
        assert_eq!(t2.1, &2);
    }
}
