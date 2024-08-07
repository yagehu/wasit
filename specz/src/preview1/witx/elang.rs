use pest::iterators::Pair;
use pest_derive::Parser;

use crate::preview1::spec::{Spec, VariantValue, WasiValue};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Stmt {
    AttrSet(AttrSet),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AttrSet {
    pub resource: String,
    pub attr:     String,
    pub value:    Expr,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Expr {
    WasiValue(WasiValue),
}

#[derive(Parser)]
#[grammar = "preview1/witx/elang.pest"]
pub struct Parser;

pub fn to_stmt(spec: &Spec, pair: Pair<'_, Rule>) -> Result<Stmt, eyre::Error> {
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
            let value = to_expr(spec, pairs.next().unwrap())?;

            Stmt::AttrSet(AttrSet {
                resource,
                attr,
                value,
            })
        },
        | _ => unreachable!("{:?}", pair),
    })
}

fn to_expr(spec: &Spec, pair: Pair<'_, Rule>) -> Result<Expr, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::s64_const => Expr::WasiValue(WasiValue::S64(
            pair.into_inner().next().unwrap().as_str().parse::<i64>()?,
        )),
        | Rule::variant_const => {
            let mut pairs = pair.into_inner();
            let type_name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();
            let case_name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();
            let ty = spec.types.get_by_key(type_name).unwrap();
            let variant_type = ty.wasi.variant().unwrap();
            let (case_idx, _case_type) = variant_type
                .cases
                .iter()
                .enumerate()
                .find(|(_i, case)| case.name == case_name)
                .unwrap();

            Expr::WasiValue(WasiValue::Variant(Box::new(VariantValue {
                case_idx,
                payload: None,
            })))
        },
        | _ => unreachable!(),
    })
}
