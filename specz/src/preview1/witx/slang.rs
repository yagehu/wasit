use std::str::FromStr;

use eyre::Context as _;
use num_bigint::BigInt;
use pest::iterators::Pair;
use pest_derive::Parser;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Term {
    Not(Box<Not>),
    And(And),
    Or(Or),

    AttrGet(Box<AttrGet>),
    Param(Param),

    FlagsGet(Box<FlagsGet>),
    IntConst(BigInt),
    IntAdd(Box<IntAdd>),
    IntLe(Box<IntLe>),

    ValueEq(Box<ValueEq>),

    VariantConst(Box<VariantConst>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Not {
    pub term: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct And {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Or {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AttrGet {
    pub target: Term,
    pub attr:   String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Param {
    pub name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsGet {
    pub target: Term,
    pub field:  String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IntAdd {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IntLe {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ValueEq {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantConst {
    pub ty:      String,
    pub case:    String,
    pub payload: Option<Term>,
}

#[derive(Parser)]
#[grammar = "preview1/witx/slang.pest"]
pub struct Parser;

pub fn to_term(pair: Pair<'_, Rule>) -> Result<Term, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::not => Term::Not(Box::new(Not {
            term: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::and => Term::And(And {
            clauses: pair
                .into_inner()
                .filter_map(|p| {
                    if p.as_rule() != Rule::comment {
                        Some(to_term(p))
                    } else {
                        None
                    }
                })
                .collect::<Result<_, _>>()?,
        }),
        | Rule::or => Term::Or(Or {
            clauses: pair
                .into_inner()
                .map(|p| to_term(p))
                .collect::<Result<_, _>>()?,
        }),
        | Rule::attr_get => {
            let mut pairs = pair.into_inner();
            let target =
                to_term(pairs.next().unwrap()).wrap_err("failed to handle @attr.get target")?;
            let attr = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::AttrGet(Box::new(AttrGet { target, attr }))
        },
        | Rule::param => Term::Param(Param {
            name: pair
                .into_inner()
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned(),
        }),
        | Rule::flags_get => {
            let mut pairs = pair.into_inner();
            let target =
                to_term(pairs.next().unwrap()).wrap_err("failed to handle @flags.get target")?;
            let field = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::FlagsGet(Box::new(FlagsGet { target, field }))
        },
        | Rule::int_add => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntAdd(Box::new(IntAdd { lhs, rhs }))
        },
        | Rule::int_le => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntLe(Box::new(IntLe { lhs, rhs }))
        },
        | Rule::num_lit => {
            let s = pair.as_str();

            Term::IntConst(BigInt::from_str(s)?)
        },
        | Rule::value_eq => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::ValueEq(Box::new(ValueEq { lhs, rhs }))
        },
        | Rule::variant_const => {
            let mut pairs = pair.into_inner();
            let ty = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let case = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let payload = match pairs.next() {
                | Some(pair) => Some(to_term(pair)?),
                | None => None,
            };

            Term::VariantConst(Box::new(VariantConst { ty, case, payload }))
        },
        | _ => panic!("{:?} {:?}", pair.as_rule(), pair.as_str()),
    })
}
