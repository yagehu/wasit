use std::collections::{HashMap, HashSet};

use gcollections::ops::Union as _;
use interval::{interval_set::ToIntervalSet as _, IntervalSet};
use num_bigint::BigInt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConstraintError {
    #[error("conflicting constraint types")]
    Conflict,
}

#[derive(Error, Debug)]
pub enum ConstraintSetError {
    #[error("conflicting constraints for {:?}", tref)]
    Conflict { tref: TypeRef },
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Constraint {
    Flags(FlagsConstraint),
    U64(IntervalSet<u64>),
}

impl Constraint {
    pub fn union(&self, other: &Self) -> Result<Self, ConstraintError> {
        let mut ret = self.clone();

        match (&mut ret, other) {
            | (Constraint::Flags(l), Constraint::Flags(r)) => *l = l.union(r),
            | (Constraint::U64(l), Constraint::U64(r)) => *l = l.union(r),
            | (Self::Flags(_), _) | (Constraint::U64(_), _) => {
                return Err(ConstraintError::Conflict)
            },
        }

        Ok(ret)
    }
}

#[derive(Default, PartialEq, Eq, Clone, Debug)]
pub struct FlagsConstraint(pub HashMap<String, HashSet<FlagConstraint>>);

impl FlagsConstraint {
    pub fn union(&self, other: &Self) -> Self {
        let mut ret = self.clone();

        for (field, constraints) in &other.0 {
            let entry = ret.0.entry(field.clone()).or_default();

            *entry = entry.union(&constraints).cloned().collect();
        }

        ret
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FlagConstraint {
    Set,
    Unset,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ConstraintSet(HashMap<TypeRef, Constraint>);

impl ConstraintSet {
    pub fn new() -> Self {
        Self(Default::default())
    }

    pub fn get(&self, tref: &TypeRef) -> Option<&Constraint> {
        self.0.get(tref)
    }

    pub fn union(&mut self, other: &Self) -> Result<(), ConstraintSetError> {
        for (tref, constraint) in &other.0 {
            match self.0.get(tref) {
                | Some(self_constraint) => self.0.insert(
                    tref.to_owned(),
                    self_constraint.union(constraint).map_err(|e| match e {
                        | ConstraintError::Conflict => ConstraintSetError::Conflict {
                            tref: tref.to_owned(),
                        },
                    })?,
                ),
                | None => self.0.insert(tref.to_owned(), constraint.to_owned()),
            };
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Program {
    pub expr: Expr,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Expr {
    And(Box<And>),
    Flag(Flag),
    U64GtU(Box<U64GtU>),
    U64LeS(Box<U64LeS>),
    U64LeU(Box<U64LeU>),
    U64LtU(Box<U64LtU>),
    Number(BigInt),
    TypeRef(TypeRef),
}

impl Expr {
    pub fn eval(&self) -> Result<ConstraintSet, ConstraintSetError> {
        Ok(match self {
            | Expr::And(e) => e.eval()?,
            | Expr::Flag(e) => e.eval(),
            | Expr::U64GtU(e) => e.eval(),
            | Expr::U64LeU(e) => e.eval(),
            | Expr::U64LtU(e) => e.eval(),
            | Expr::Number(_) => todo!(),
            | Expr::TypeRef(_) => todo!(),
        })
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct And {
    pub clauses: Vec<Expr>,
}

impl And {
    fn eval(&self) -> Result<ConstraintSet, ConstraintSetError> {
        let mut cset = ConstraintSet::new();

        for clause in &self.clauses {
            let clause_cset = clause.eval()?;

            cset.union(&clause_cset)?;
        }

        Ok(cset)
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Flag {
    pub tref:       TypeRef,
    pub field:      String,
    pub constraint: FlagConstraint,
}

impl Flag {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();
        let mut set = HashSet::new();
        let mut constraint = HashMap::new();

        set.insert(self.constraint);
        constraint.insert(self.field.clone(), set);
        cset.0.insert(
            self.tref.clone(),
            Constraint::Flags(FlagsConstraint(constraint)),
        );

        cset
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct U64GtU {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl U64GtU {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();

        match (&self.lhs, &self.rhs) {
            | (Expr::Number(num), Expr::TypeRef(tref)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(vec![(0, u64::try_from(num).unwrap() - 1)].to_interval_set()),
                );
            },
            | (Expr::TypeRef(tref), Expr::Number(num)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(
                        vec![(u64::try_from(num).unwrap() + 1, u64::MAX - 1)].to_interval_set(),
                    ),
                );
            },
            | _ => panic!("u64.gt"),
        }

        cset
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct U64LeU {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl U64LeU {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();

        match (&self.lhs, &self.rhs) {
            | (Expr::Number(num), Expr::TypeRef(tref)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(
                        vec![(u64::try_from(num).unwrap(), u64::MAX - 1)].to_interval_set(),
                    ),
                );
            },
            | (Expr::TypeRef(tref), Expr::Number(num)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(vec![(0, u64::try_from(num).unwrap())].to_interval_set()),
                );
            },
            | _ => panic!("u64.lt"),
        }

        cset
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct U64LtU {
    pub lhs: Expr,
    pub rhs: Expr,
}

impl U64LtU {
    fn eval(&self) -> ConstraintSet {
        let mut cset = ConstraintSet::new();

        match (&self.lhs, &self.rhs) {
            | (Expr::Number(num), Expr::TypeRef(tref)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(
                        vec![(u64::try_from(num).unwrap() + 1, u64::MAX - 1)].to_interval_set(),
                    ),
                );
            },
            | (Expr::TypeRef(tref), Expr::Number(num)) => {
                cset.0.insert(
                    tref.to_owned(),
                    Constraint::U64(vec![(0, u64::try_from(num).unwrap() - 1)].to_interval_set()),
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
