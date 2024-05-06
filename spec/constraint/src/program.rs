use core::fmt;
use std::collections::HashMap;

use gcollections::ops::{Bounded as _, Union as _};
use interval::{
    interval_set::ToIntervalSet as _,
    ops::{Range, Width},
    Interval,
    IntervalSet,
};
use num_bigint::BigInt;
use num_traits::Num;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ConstraintSet(HashMap<TypeRef, IntervalSet<u64>>);

impl ConstraintSet {
    pub fn new() -> Self {
        Self(Default::default())
    }

    pub fn get<T>(&self, tref: &TypeRef) -> IntervalSet<T>
    where
        T: Width + Num + TryFrom<u64>,
        <T as TryFrom<u64>>::Error: fmt::Debug,
    {
        self.0
            .get(tref)
            .map(|iset| {
                let mut new = vec![].to_interval_set();

                for i in iset.iter() {
                    new.extend([Interval::new(
                        T::try_from(i.lower()).unwrap(),
                        T::try_from(i.upper()).unwrap(),
                    )]);
                }

                new
            })
            .unwrap_or(vec![(T::min_value(), T::max_value())].to_interval_set())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Program {
    pub expr: Expr,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Expr {
    And(Box<And>),
    U64Gt(Box<U64Gt>),
    U64Le(Box<U64Le>),
    U64Lt(Box<U64Lt>),
    Number(BigInt),
    TypeRef(TypeRef),
}

impl Expr {
    pub fn eval(&self) -> ConstraintSet {
        match self {
            | Expr::And(e) => e.eval(),
            | Expr::U64Gt(e) => e.eval(),
            | Expr::U64Lt(e) => e.eval(),
            | Expr::Number(_) => todo!(),
            | Expr::TypeRef(_) => todo!(),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct And {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl And {
    fn eval(&self) -> ConstraintSet {
        let mut l = self.lhs.eval();
        let r = self.rhs.eval();

        for (tref, r_set) in r.0 {
            match l.0.get(&tref) {
                | Some(l_set) => l.0.insert(tref, l_set.union(&r_set)),
                | None => l.0.insert(tref, r_set),
            };
        }

        l
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct U64Gt {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl U64Gt {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();

        match (&self.lhs, &self.rhs) {
            | (Expr::Number(num), Expr::TypeRef(tref)) => {
                cset.0.insert(
                    tref.to_owned(),
                    vec![(0, u64::try_from(num).unwrap() - 1)].to_interval_set(),
                );
            },
            | (Expr::TypeRef(tref), Expr::Number(num)) => {
                cset.0.insert(
                    tref.to_owned(),
                    vec![(u64::try_from(num).unwrap() + 1, u64::MAX - 1)].to_interval_set(),
                );
            },
            | _ => panic!("u64.gt"),
        }

        cset
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct U64Lt {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl U64Lt {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();

        match (&self.lhs, &self.rhs) {
            | (Expr::Number(num), Expr::TypeRef(tref)) => {
                cset.0.insert(
                    tref.to_owned(),
                    vec![(u64::try_from(num).unwrap() + 1, u64::MAX - 1)].to_interval_set(),
                );
            },
            | (Expr::TypeRef(tref), Expr::Number(num)) => {
                cset.0.insert(
                    tref.to_owned(),
                    vec![(0, u64::try_from(num).unwrap() - 1)].to_interval_set(),
                );
            },
            | _ => panic!("u64.lt"),
        }

        cset
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub enum TypeRef {
    Param { name: String },
    Result { name: String },
}
