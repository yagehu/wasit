use std::str::FromStr;

use eyre::Context as _;
use num_bigint::BigInt;
use pest::iterators::Pair;
use pest_derive::Parser;

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum Term {
    Not(Box<Not>),
    And(And),
    Or(Or),

    RecordFieldGet(Box<RecordFieldGet>),
    Param(Param),

    FlagsGet(Box<FlagsGet>),
    IntConst(BigInt),
    IntAdd(Box<IntAdd>),
    IntLe(Box<IntLe>),

    ValueEq(Box<ValueEq>),

    VariantConst(Box<VariantConst>),

    NoNonExistentDirBacktrack(Box<NoNonExistentDirBacktrack>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Not {
    pub(crate) term: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct And {
    pub(crate) clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Or {
    pub(crate) clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct RecordFieldGet {
    pub(crate) target: Term,
    pub(crate) member: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Param {
    pub(crate) name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct FlagsGet {
    pub(crate) target: Term,
    pub(crate) field:  String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntAdd {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntLe {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct ValueEq {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct VariantConst {
    pub(crate) ty:      String,
    pub(crate) case:    String,
    pub(crate) payload: Option<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct NoNonExistentDirBacktrack {
    pub(crate) fd_param:   String,
    pub(crate) path_param: String,
}

#[derive(Parser)]
#[grammar = "spec/witx/ilang.pest"]
pub(super) struct Parser;

pub(super) fn to_term(pair: Pair<'_, Rule>) -> Result<Term, eyre::Error> {
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
        | Rule::record_field_get => {
            let mut pairs = pair.into_inner();
            let target = to_term(pairs.next().unwrap())
                .wrap_err("failed to handle @record.field.get target")?;
            let attr = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::RecordFieldGet(Box::new(RecordFieldGet {
                target,
                member: attr,
            }))
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
        | Rule::no_nonexistent_dir_backtrack => {
            let mut pairs = pair.into_inner();
            let fd_param = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_string();
            let path_param = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_string();

            Term::NoNonExistentDirBacktrack(Box::new(NoNonExistentDirBacktrack {
                fd_param,
                path_param,
            }))
        },
        | _ => panic!("{:?} {:?}", pair.as_rule(), pair.as_str()),
    })
}
