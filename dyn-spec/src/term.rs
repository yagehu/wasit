use wazzi_spec::parsers::wazzi_preview1::{self, Keyword};

use crate::{ast::Idx, wasi, Environment};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Variable {
    Attr(Attr),
    Param(Param),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Term {
    Conj(Conj),
    Disj(Disj),

    Attr(Attr),
    Param(Param),

    Value(wasi::Value),

    ValueEq(Box<ValueEq>),
    I64Add(Box<I64Add>),
    I64Ge(Box<I64Ge>),
    I64Le(Box<I64Le>),
}

impl Term {
    pub fn from_preview1_annotation(env: &Environment, annot: wazzi_preview1::Annotation) -> Self {
        if annot.span.name != "input-contract" {
            panic!();
        }

        if annot.exprs.len() > 1 {
            panic!("more than one expression");
        }

        Self::from_preview1_expr(env, annot.exprs.get(0).unwrap())
    }

    fn from_preview1_expr(env: &Environment, expr: &wazzi_preview1::Expr) -> Self {
        match expr {
            | wazzi_preview1::Expr::Annotation(_) => todo!(),
            | wazzi_preview1::Expr::SymbolicIdx(_) => todo!(),
            | wazzi_preview1::Expr::Keyword(_) => todo!(),
            | wazzi_preview1::Expr::NumLit(_) => todo!(),
            | wazzi_preview1::Expr::SExpr(exprs) => {
                if let Some(annot) = exprs.first().unwrap().annotation() {
                    match annot.name {
                        | "and" => {
                            return Self::Conj(Conj {
                                clauses: exprs[1..]
                                    .iter()
                                    .map(|e| Self::from_preview1_expr(env, e))
                                    .collect(),
                            })
                        },
                        | "or" => {
                            return Self::Disj(Disj {
                                clauses: exprs[1..]
                                    .iter()
                                    .map(|e| Self::from_preview1_expr(env, e))
                                    .collect(),
                            })
                        },
                        | "attr.get" => {
                            let param_exprs = exprs[1].sexpr().unwrap();

                            if param_exprs[0].keyword().unwrap().keyword
                                != wazzi_preview1::Keyword::Param
                            {
                                panic!();
                            }

                            let param = param_exprs[1].symbolic_idx().unwrap().name().to_owned();
                            let name = exprs[2].symbolic_idx().unwrap().name().to_owned();

                            return Self::Attr(Attr { param, name });
                        },
                        | "value.eq" => {
                            return Self::ValueEq(Box::new(ValueEq {
                                lhs: Self::from_preview1_expr(env, &exprs[1]),
                                rhs: Self::from_preview1_expr(env, &exprs[2]),
                            }))
                        },
                        | _ => panic!(),
                    }
                }

                if let Some(span) = exprs.first().unwrap().keyword() {
                    match span.keyword {
                        | Keyword::Param => {
                            return Self::Param(Param {
                                name: exprs
                                    .get(1)
                                    .unwrap()
                                    .symbolic_idx()
                                    .unwrap()
                                    .name()
                                    .to_owned(),
                            })
                        },
                        | Keyword::Enum => {
                            let variant_name = exprs
                                .get(1)
                                .unwrap()
                                .symbolic_idx()
                                .unwrap()
                                .name()
                                .to_owned();
                            let case_name = exprs
                                .get(2)
                                .unwrap()
                                .symbolic_idx()
                                .unwrap()
                                .name()
                                .to_owned();
                            let resource_type = env
                                .resource_types
                                .get(&Idx::Symbolic(variant_name.clone()))
                                .unwrap();
                            let variant = match &resource_type.wasi_type {
                                | wasi::Type::Variant(variant) => variant,
                                | _ => panic!("not a variant"),
                            };

                            return Self::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                                case_idx: variant
                                    .cases
                                    .iter()
                                    .enumerate()
                                    .find(|(_, case)| case.name == case_name)
                                    .unwrap()
                                    .0,
                                case_name,
                                payload: None,
                            })));
                        },
                        | Keyword::I64Add => {
                            return Self::I64Add(Box::new(I64Add {
                                l: Self::from_preview1_expr(env, exprs.get(1).unwrap()),
                                r: Self::from_preview1_expr(env, exprs.get(2).unwrap()),
                            }));
                        },
                        | Keyword::I64Const => {
                            return Self::Value(wasi::Value::I64(
                                exprs
                                    .get(1)
                                    .unwrap()
                                    .num_lit()
                                    .unwrap()
                                    .0
                                    .parse::<i64>()
                                    .unwrap(),
                            ))
                        },
                        | Keyword::I64LeS => {
                            return Self::I64Le(Box::new(I64Le {
                                lhs: Self::from_preview1_expr(env, exprs.get(1).unwrap()),
                                rhs: Self::from_preview1_expr(env, exprs.get(2).unwrap()),
                            }));
                        },
                        | Keyword::I64GeS => {
                            return Self::I64Ge(Box::new(I64Ge {
                                lhs: Self::from_preview1_expr(env, exprs.get(1).unwrap()),
                                rhs: Self::from_preview1_expr(env, exprs.get(2).unwrap()),
                            }));
                        },
                        | _ => panic!("{:?}", span),
                    }
                }

                todo!("{:?}", exprs)
            },
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Conj {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Disj {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Attr {
    pub param: String,
    pub name:  String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Param {
    pub name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ValueEq {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct I64Add {
    pub l: Term,
    pub r: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct I64Ge {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct I64Le {
    pub lhs: Term,
    pub rhs: Term,
}
