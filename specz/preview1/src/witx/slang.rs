use std::str::FromStr;

use eyre::Context as _;
use num_bigint::BigInt;
use pest::iterators::Pair;
use pest_derive::Parser;

use wazzi_specz_wasi::{term, Term};

#[derive(Parser)]
#[grammar = "witx/slang.pest"]
pub struct Parser;

pub fn to_term(pair: Pair<'_, Rule>) -> Result<Term, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::not => Term::Not(Box::new(term::Not {
            term: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::and => Term::And(term::And {
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
        | Rule::or => Term::Or(term::Or {
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

            Term::AttrGet(Box::new(term::AttrGet { target, attr }))
        },
        | Rule::param => Term::Param(term::Param {
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
            let r#type = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let field = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::FlagsGet(Box::new(term::FlagsGet {
                target,
                r#type,
                field,
            }))
        },
        | Rule::int_add => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntAdd(Box::new(term::IntAdd { lhs, rhs }))
        },
        | Rule::int_le => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntLe(Box::new(term::IntLe { lhs, rhs }))
        },
        | Rule::num_lit => {
            let s = pair.as_str();

            Term::IntConst(BigInt::from_str(s)?)
        },
        | Rule::value_eq => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::ValueEq(Box::new(term::ValueEq { lhs, rhs }))
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

            Term::VariantConst(Box::new(term::VariantConst { ty, case, payload }))
        },
        | _ => panic!("{:?} {:?}", pair.as_rule(), pair.as_str()),
    })
}
