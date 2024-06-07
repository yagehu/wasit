use pest::iterators::Pair;
use pest_derive::Parser;

use wazzi_specz_wasi::{effects, Spec, VariantValue, WasiValue};

#[derive(Parser)]
#[grammar = "witx/elang.pest"]
pub struct Parser;

pub fn to_stmt(spec: &Spec, pair: Pair<'_, Rule>) -> Result<effects::Stmt, eyre::Error> {
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

            effects::Stmt::AttrSet(effects::AttrSet {
                resource,
                attr,
                value,
            })
        },
        | _ => unreachable!("{:?}", pair),
    })
}

fn to_expr(spec: &Spec, pair: Pair<'_, Rule>) -> Result<effects::Expr, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::s64_const => effects::Expr::WasiValue(WasiValue::S64(
            pair.into_inner().next().unwrap().as_str().parse::<i64>()?,
        )),
        | Rule::variant_const => {
            let mut pairs = pair.into_inner();
            let type_name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();
            let case_name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();
            let ty = spec
                .types
                .get(*spec.types_map.get(type_name).unwrap())
                .unwrap();
            let variant_type = ty.wasi.variant().unwrap();
            let (case_idx, _case_type) = variant_type
                .cases
                .iter()
                .enumerate()
                .find(|(_i, case)| case.name == case_name)
                .unwrap();

            effects::Expr::WasiValue(WasiValue::Variant(Box::new(VariantValue {
                case_idx,
                payload: None,
            })))
        },
        | _ => unreachable!(),
    })
}
