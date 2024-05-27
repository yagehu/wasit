use pest::iterators::Pair;
use pest_derive::Parser;

use wazzi_specz_wasi::{effects, WasiValue};

#[derive(Parser)]
#[grammar = "witx/elang.pest"]
pub struct Parser;

pub fn to_stmt(pair: Pair<'_, Rule>) -> Result<effects::Stmt, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::attr_set => {
            let mut pairs = pair.into_inner();
            let resource = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let attr = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let value = to_expr(pairs.next().unwrap())?;

            effects::Stmt::AttrSet(effects::AttrSet {
                resource,
                attr,
                value,
            })
        },
        | _ => unreachable!("{:?}", pair),
    })
}

fn to_expr(pair: Pair<'_, Rule>) -> Result<effects::Expr, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::s64_const => effects::Expr::WasiValue(WasiValue::S64(
            pair.into_inner().next().unwrap().as_str().parse::<i64>()?,
        )),
        | _ => unreachable!(),
    })
}
