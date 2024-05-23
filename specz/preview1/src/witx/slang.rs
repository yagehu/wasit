use eyre::Context as _;
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
                .map(|p| to_term(p))
                .collect::<Result<_, _>>()?,
        }),
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
        | _ => panic!("{:?}", pair.as_rule()),
    })
}
