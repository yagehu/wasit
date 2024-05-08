use std::collections::HashMap;

use thiserror::Error;

use crate::{ast, wasi, Interface};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Variable {
    value:      wasi::Value,
    attributes: HashMap<String, wasi::Value>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Context {
    params:     Vec<Variable>,
    results:    Vec<Variable>,
    param_map:  HashMap<String, usize>,
    result_map: HashMap<String, usize>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            params:     Default::default(),
            results:    Default::default(),
            param_map:  Default::default(),
            result_map: Default::default(),
        }
    }

    pub fn get_param(&self, name: &str) -> Option<&Variable> {
        let i = *self.param_map.get(name)?;

        self.params.get(i)
    }

    pub fn params(&self) -> impl Iterator<Item = &Variable> {
        self.params.iter()
    }

    pub fn push_param(&mut self, name: String, value: Variable) {
        self.params.push(value);
        self.param_map.insert(name, self.params.len() - 1);
    }

    pub fn push_result(&mut self, name: String, value: Variable) {
        self.results.push(value);
        self.result_map.insert(name, self.results.len() - 1);
    }

    pub fn evaluate_spec(
        &mut self,
        interface: &Interface,
        spec: &ast::Spec,
    ) -> Result<wasi::Value, Error> {
        let mut value = wasi::Value::Unit;

        for expr in &spec.exprs {
            value = self.evaluate(interface, expr)?;
        }

        Ok(value)
    }

    pub fn evaluate(
        &mut self,
        interface: &Interface,
        expr: &ast::Expr,
    ) -> Result<wasi::Value, Error> {
        Ok(match expr {
            | ast::Expr::AttrSet(attr_set) => {
                let value = self.resolve_value(&attr_set.value)?;
                let var = match &attr_set.var {
                    | ast::VarRef::Param(idx) => self.param_mut(idx),
                    | ast::VarRef::Result(idx) => self.result_mut(idx),
                }?;
                let attr =
                    var.attributes
                        .get_mut(&attr_set.attr)
                        .ok_or_else(|| Error::UnknownRef {
                            ty:  RefType::Attribute,
                            idx: ast::Idx::Symbolic(attr_set.attr.clone()),
                        })?;

                *attr = value.clone();

                value
            },
            | ast::Expr::Enum(e) => {
                let ty = interface
                    .get_type(&e.typename)
                    .ok_or_else(|| Error::UnknownRef {
                        ty:  RefType::Type,
                        idx: e.typename.clone(),
                    })?;
                let variant = ty.variant().ok_or_else(|| Error::InterfaceType {
                    expected: wasi::TopLevelType::Variant,
                    actual:   ty.clone(),
                })?;
                let (case_idx, case) = variant
                    .cases
                    .iter()
                    .enumerate()
                    .find(|(i, case)| match &e.variant {
                        | ast::Idx::Symbolic(name) => &case.name == name,
                        | ast::Idx::Numeric(idx) => i == idx,
                    })
                    .unwrap();

                wasi::Value::Variant(Box::new(wasi::Variant {
                    case_idx,
                    case_name: case.name.clone(),
                    payload: None,
                }))
            },
            | ast::Expr::If(if_) => {
                let cond = self.evaluate(interface, &if_.cond)?;
                let ok = cond.bool_().ok_or_else(|| Error::Type {
                    expected: wasi::Type::Bool,
                    actual:   cond,
                })?;
                let mut value = wasi::Value::Unit;

                if ok {
                    for expr in &if_.then {
                        value = self.evaluate(interface, expr)?;
                    }
                }

                value
            },
            | ast::Expr::Param(idx) => self.param(idx)?.value.clone(),
            | ast::Expr::Result(idx) => self.result(idx)?.value.clone(),
            | ast::Expr::ValueEq(value_eq) => {
                let l = self.evaluate(interface, &value_eq.lhs)?;
                let r = self.evaluate(interface, &value_eq.rhs)?;

                wasi::Value::Bool(l == r)
            },
        })
    }

    fn resolve_value(&self, value: &ast::Value) -> Result<wasi::Value, Error> {
        Ok(match value {
            | &ast::Value::I64(i) => wasi::Value::I64(i),
            | &ast::Value::U64(i) => wasi::Value::U64(i),
            | ast::Value::Param(idx) => self.param(idx)?.value.clone(),
            | ast::Value::Result(idx) => self.result(idx)?.value.clone(),
        })
    }

    fn param(&self, idx: &ast::Idx) -> Result<&Variable, Error> {
        let i = match idx {
            | ast::Idx::Symbolic(name) => {
                *self.param_map.get(name).ok_or_else(|| Error::UnknownRef {
                    ty:  RefType::Param,
                    idx: idx.clone(),
                })?
            },
            | &ast::Idx::Numeric(i) => i,
        };

        self.params.get(i).ok_or_else(|| Error::UnknownRef {
            ty:  RefType::Param,
            idx: idx.clone(),
        })
    }

    fn result(&self, idx: &ast::Idx) -> Result<&Variable, Error> {
        let i = match idx {
            | ast::Idx::Symbolic(name) => {
                *self.result_map.get(name).ok_or_else(|| Error::UnknownRef {
                    ty:  RefType::Result,
                    idx: idx.clone(),
                })?
            },
            | &ast::Idx::Numeric(i) => i,
        };

        self.results.get(i).ok_or_else(|| Error::UnknownRef {
            ty:  RefType::Result,
            idx: idx.clone(),
        })
    }

    fn param_mut(&mut self, idx: &ast::Idx) -> Result<&mut Variable, Error> {
        let i = match idx {
            | ast::Idx::Symbolic(name) => {
                *self.param_map.get(name).ok_or_else(|| Error::UnknownRef {
                    ty:  RefType::Param,
                    idx: idx.clone(),
                })?
            },
            | &ast::Idx::Numeric(i) => i,
        };

        self.params.get_mut(i).ok_or_else(|| Error::UnknownRef {
            ty:  RefType::Param,
            idx: idx.clone(),
        })
    }

    fn result_mut(&mut self, idx: &ast::Idx) -> Result<&mut Variable, Error> {
        let i = match idx {
            | ast::Idx::Symbolic(name) => {
                *self.result_map.get(name).ok_or_else(|| Error::UnknownRef {
                    ty:  RefType::Result,
                    idx: idx.clone(),
                })?
            },
            | &ast::Idx::Numeric(i) => i,
        };

        self.results.get_mut(i).ok_or_else(|| Error::UnknownRef {
            ty:  RefType::Result,
            idx: idx.clone(),
        })
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("unknown {:?} reference: {:?}", ty, idx)]
    UnknownRef { ty: RefType, idx: ast::Idx },

    #[error("wrong type, expected {:?} but got value {:?}", expected, actual)]
    Type {
        expected: wasi::Type,
        actual:   wasi::Value,
    },

    #[error("wrong interface type, expected {:?} but got {:?}", expected, actual)]
    InterfaceType {
        expected: wasi::TopLevelType,
        actual:   wasi::Type,
    },
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum RefType {
    Type,
    Attribute,
    Param,
    Result,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ast::*,
        wasi::{CaseType, Type, Value, Variant, VariantType},
    };

    #[test]
    fn set_attr() {
        let mut ctx = Context::new();
        let interface = Interface::new();

        ctx.push_param(
            "fd".to_owned(),
            Variable {
                value:      Value::U32(3),
                attributes: HashMap::from([("offset".to_owned(), Value::U64(0))]),
            },
        );
        ctx.evaluate(
            &interface,
            &ast::Expr::AttrSet(ast::AttrSet {
                var:   ast::VarRef::Param(ast::Idx::Symbolic("fd".to_owned())),
                attr:  "offset".to_owned(),
                value: ast::Value::U64(10),
            }),
        )
        .expect("failed to evaluate spec");

        assert_eq!(
            ctx.get_param("fd")
                .unwrap()
                .attributes
                .get("offset")
                .unwrap(),
            &Value::U64(10)
        );
    }

    #[test]
    fn if_then() {
        let mut ctx = Context::new();
        let mut interface = Interface::new();

        interface.push_type(
            "errno".to_owned(),
            Type::Variant(VariantType {
                cases: vec![CaseType {
                    name:    "success".to_owned(),
                    payload: None,
                }],
            }),
        );
        ctx.push_param(
            "fd".to_owned(),
            Variable {
                value:      Value::U32(3),
                attributes: HashMap::from([("offset".to_owned(), Value::U64(0))]),
            },
        );
        ctx.push_result(
            "errno".to_owned(),
            Variable {
                value:      Value::Variant(Box::new(Variant {
                    case_idx:  0,
                    case_name: "success".to_owned(),
                    payload:   None,
                })),
                attributes: Default::default(),
            },
        );
        ctx.evaluate(
            &interface,
            &Expr::If(Box::new(If {
                cond: Expr::ValueEq(Box::new(ValueEq {
                    lhs: Expr::Result(Idx::Symbolic("errno".to_owned())),
                    rhs: Expr::Enum(Enum {
                        typename: Idx::Symbolic("errno".to_owned()),
                        variant:  Idx::Symbolic("success".to_owned()),
                    }),
                })),
                then: vec![ast::Expr::AttrSet(ast::AttrSet {
                    var:   ast::VarRef::Param(ast::Idx::Symbolic("fd".to_owned())),
                    attr:  "offset".to_owned(),
                    value: ast::Value::U64(10),
                })],
            })),
        )
        .expect("failed to evaluate spec");

        assert_eq!(
            ctx.get_param("fd")
                .unwrap()
                .attributes
                .get("offset")
                .unwrap(),
            &Value::U64(10)
        );
    }
}
