use pest::iterators::Pair;
use pest_derive::Parser;

use crate::spec::{Spec, VariantValue, WasiValue};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Stmt {
    RecordFieldSet(RecordFieldSet),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordFieldSet {
    pub result: String,
    pub field:  String,
    pub value:  Expr,
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
#[grammar = "spec/witx/olang.pest"]
pub(super) struct Parser;

pub(super) fn to_stmt(spec: &Spec, pair: Pair<'_, Rule>) -> Result<Stmt, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::record_field_set => {
            let mut pairs = pair.into_inner();
            let resource = pairs
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
            let value = to_expr(spec, pairs.next().unwrap())?;

            Stmt::RecordFieldSet(RecordFieldSet {
                result: resource,
                field,
                value,
            })
        },
        | _ => unreachable!("{:?}", pair),
    })
}

fn to_expr(spec: &Spec, pair: Pair<'_, Rule>) -> Result<Expr, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::u64_const => Expr::WasiValue(WasiValue::U64(
            pair.into_inner().next().unwrap().as_str().parse::<u64>()?,
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
