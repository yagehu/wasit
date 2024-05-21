use eyre::Context as _;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::{term, Term};

#[derive(Parser)]
#[grammar = "witx/slang.pest"]
pub struct Parser;

pub fn to_term(pair: Pair<'_, Rule>) -> Result<Term, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::not => Term::Not(Box::new(term::Not {
            term: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::and => {
            let mut clauses = Vec::new();

            for pair in pair.into_inner() {
                clauses.push(to_term(pair)?);
            }

            Term::And(term::And { clauses })
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
            let field = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::FlagsGet(Box::new(term::FlagsGet { target, field }))
        },
        | _ => panic!("{:?}", pair.as_rule()),
    })
}
