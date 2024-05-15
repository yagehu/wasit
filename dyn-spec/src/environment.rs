use std::collections::HashMap;

use arbitrary::Unstructured;
use thiserror::Error;
use wazzi_spec::parsers::wazzi_preview1;

use crate::{
    ast::Idx,
    resource_ctx::{Resource, ResourceContext},
    term,
    wasi,
    IndexSpace,
    Term,
};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Variable {
    Resource(Resource),
    Value(wasi::Value),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub params:         Vec<FunctionParam>,
    pub input_contract: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name:              String,
    pub resource_type_idx: usize,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ResourceType {
    pub wasi_type:  wasi::Type,
    pub attributes: HashMap<String, wasi::Type>,
    pub fungible:   bool,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    functions:                 IndexSpace<Function>,
    pub(crate) resource_types: IndexSpace<ResourceType>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            functions:      Default::default(),
            resource_types: Default::default(),
        }
    }

    pub fn call(
        &mut self,
        u: &mut Unstructured,
        resources: &ResourceContext,
        function_name: &str,
    ) -> Result<Solution, Error> {
        let function = self
            .functions
            .get(&Idx::Symbolic(function_name.to_owned()))
            .unwrap();
        let solution = self
            .solve(
                u,
                &function.input_contract,
                resources,
                &Idx::Symbolic(function_name.to_owned()),
            )
            .ok_or_else(|| Error::NoSolution {
                function: function_name.to_owned(),
            })?;

        Ok(solution)
    }

    pub fn ingest_preview1_spec(&mut self, module: wazzi_preview1::Module) {
        for decl in &module.decls {
            match decl {
                | wazzi_preview1::Decl::Typename(typename) => {
                    self.ingest_preview1_typename(typename);
                },
                | wazzi_preview1::Decl::Function(function) => {
                    self.functions.push(
                        Some(function.name.to_owned()),
                        Function {
                            params:         function
                                .params
                                .iter()
                                .map(|p| {
                                    let resource_type_idx = match &p.tref {
                                        | &wazzi_preview1::TypeRef::Numeric(i) => i as usize,
                                        | wazzi_preview1::TypeRef::Symbolic(id) => self
                                            .resource_types
                                            .resolve_idx(&Idx::Symbolic(id.name().to_owned()))
                                            .expect(&format!("{}", id.name())),
                                        | wazzi_preview1::TypeRef::Type(_) => todo!(),
                                    };

                                    FunctionParam {
                                        name: p.name.name().to_owned(),
                                        resource_type_idx,
                                    }
                                })
                                .collect(),
                            input_contract: function
                                .annotations
                                .iter()
                                .find(|annot| annot.span.name == "input-contract")
                                .map(|annot| Term::from_preview1_annotation(self, annot.to_owned()))
                                .unwrap(),
                        },
                    );
                },
            }
        }
    }

    fn ingest_preview1_typename(&mut self, typename: &wazzi_preview1::Typename) {
        let wasi_type = match &typename.tref {
            | &wazzi_preview1::TypeRef::Numeric(i) => self
                .resource_types
                .get(&Idx::Numeric(i as usize))
                .unwrap()
                .wasi_type
                .clone(),
            | wazzi_preview1::TypeRef::Symbolic(name) => self
                .resource_types
                .get(&Idx::Symbolic(name.name().to_owned()))
                .unwrap()
                .wasi_type
                .clone(),
            | wazzi_preview1::TypeRef::Type(ty) => wasi::Type::from_preview1_type(ty),
        };

        self.resource_types.push(
            typename.id.as_ref().map(|id| id.name().to_owned()),
            ResourceType {
                wasi_type,
                attributes: typename
                    .annotations
                    .iter()
                    .filter(|annot| annot.span.name == "attribute")
                    .map(|annot| {
                        let name = annot
                            .exprs
                            .first()
                            .unwrap()
                            .symbolic_idx()
                            .unwrap()
                            .name()
                            .to_owned();
                        let ty_expr = annot.exprs.get(1).unwrap();

                        (name, self.preview1_expr_as_wasi_type(ty_expr))
                    })
                    .collect(),
                fungible: true,
            },
        );
    }

    fn preview1_expr_as_wasi_type(&self, expr: &wazzi_preview1::Expr) -> wasi::Type {
        match expr {
            | wazzi_preview1::Expr::Annotation(_) => todo!(),
            | wazzi_preview1::Expr::SymbolicIdx(id) => self
                .resource_types
                .get(&Idx::Symbolic(id.name().to_owned()))
                .expect(&format!("{}", id.name()))
                .wasi_type
                .clone(),
            | wazzi_preview1::Expr::Keyword(_) => todo!(),
            | wazzi_preview1::Expr::NumLit(_) => todo!(),
            | wazzi_preview1::Expr::SExpr(_) => todo!(),
        }
    }

    pub fn functions_mut(&mut self) -> &mut IndexSpace<Function> {
        &mut self.functions
    }

    pub fn resource_types_mut(&mut self) -> &mut IndexSpace<ResourceType> {
        &mut self.resource_types
    }

    pub fn solve(
        &self,
        u: &mut Unstructured,
        t: &Term,
        resources: &ResourceContext,
        function: &Idx,
    ) -> Option<Solution> {
        let mut solution = Vec::new();

        if !self.solve_helper(u, t, resources, function, &mut solution) {
            return None;
        }

        let function = self.functions.get(function).unwrap();
        let mut params_in_order = Vec::with_capacity(function.params.len());

        for param in function.params.iter() {
            if let Some(param) = solution.iter().find(|p| p.name == param.name) {
                params_in_order.push(param.to_owned());
            } else {
                let resource_type = self
                    .resource_types
                    .get(&Idx::Numeric(param.resource_type_idx))
                    .unwrap();

                params_in_order.push(Param {
                    name:  param.name.clone(),
                    inner: ParamInner::Value(
                        wasi::Value::arbitrary(&resource_type.wasi_type, u).unwrap(),
                    ),
                });
            }
        }

        Some(Solution::new(params_in_order))
    }

    fn solve_helper(
        &self,
        u: &mut Unstructured,
        t: &Term,
        resources: &ResourceContext,
        function_idx: &Idx,
        solution: &mut Vec<Param>,
    ) -> bool {
        let var = match self.free_variable(t) {
            | Some(var) => var,
            | None => match t {
                | &Term::Value {
                    ty: _,
                    inner: wasi::Value::Bool(b),
                } => return b,
                | Term::Value { .. } => panic!("terminal has wrong type {:?}", t),
                | _ => panic!("failed to evaluate term without variables"),
            },
        };
        let param_name = match &var {
            | term::Variable::Attr(attr) => &attr.param,
            | term::Variable::Param(param) => &param.name,
        }
        .to_owned();
        let function = self.functions.get(function_idx).unwrap();
        let param = function
            .params
            .iter()
            .find(|p| p.name == param_name)
            .unwrap();
        let resource_type = self
            .resource_types
            .get(&Idx::Numeric(param.resource_type_idx))
            .unwrap();
        let mut resource_pool = resources.get_by_type(param.resource_type_idx);

        // Shuffle the resource pool.

        let mut to_permute = resource_pool.as_mut_slice();

        while to_permute.len() > 1 {
            let idx = u.choose_index(to_permute.len()).unwrap();

            to_permute.swap(0, idx);
            to_permute = &mut to_permute[1..];
        }

        // `resource_pool` is shuffled.

        let mut i = 0;
        let mut value = self.generate_value(u, &resource_type.wasi_type);
        let search_start = value.clone();

        loop {
            if !resource_pool.is_empty() && i < resource_pool.len() && u.ratio(95, 100).unwrap() {
                // Most of the time, try the resource.

                let (resource_idx, resource) = *resource_pool.get(i).unwrap();

                solution.push(Param {
                    name:  param.name.clone(),
                    inner: ParamInner::Resource(resource_idx),
                });

                let guess =
                    self.guess_variable(u, &var, Variable::Resource(resource.clone()), t, function);
                let solved = self.solve_helper(u, &guess, resources, function_idx, solution);

                if solved {
                    return true;
                }

                i += 1;
                solution.pop();
            } else {
                if !resource_type.fungible {
                    break;
                }

                solution.push(Param {
                    name:  param.name.clone(),
                    inner: ParamInner::Value(value.clone()),
                });

                let guess =
                    self.guess_variable(u, &var, Variable::Value(value.clone()), t, function);
                let solved = self.solve_helper(u, &guess, resources, function_idx, solution);

                if solved {
                    return true;
                }

                solution.pop();

                value = match (&value, &resource_type.wasi_type) {
                    | (wasi::Value::Variant(variant), wasi::Type::Variant(variant_type)) => {
                        let ncases = variant_type.cases.len();
                        let mut case_idx = variant.case_idx + 1;

                        if case_idx == ncases {
                            case_idx = 0;
                        }

                        wasi::Value::Variant(Box::new(wasi::Variant {
                            case_idx,
                            payload: None,
                        }))
                    },
                    | (wasi::Value::S64(i), _) => wasi::Value::S64(i.wrapping_add(1)),
                    | _ => panic!(),
                };

                if value == search_start {
                    break;
                }
            }
        }

        return false;
    }

    fn generate_value(&self, u: &mut Unstructured, ty: &wasi::Type) -> wasi::Value {
        match ty {
            | wasi::Type::Unit => todo!(),
            | wasi::Type::Bool => wasi::Value::Bool(u.arbitrary().unwrap()),
            | wasi::Type::S64 => wasi::Value::S64(u.arbitrary().unwrap()),
            | wasi::Type::U32 => todo!(),
            | wasi::Type::U64 => todo!(),
            | wasi::Type::Handle => wasi::Value::Handle(u.arbitrary().unwrap()),
            | wasi::Type::Flags(flags) => {
                let mut fields = Vec::with_capacity(flags.fields.len());

                for _field in flags.fields.iter() {
                    fields.push(u.arbitrary().unwrap())
                }

                wasi::Value::Flags(wasi::Flags {
                    repr: flags.repr.into(),
                    fields,
                })
            },
            | wasi::Type::Variant(variant) => {
                let cases = variant.cases.iter().enumerate().collect::<Vec<_>>();
                let &(case_idx, case) = u.choose(&cases).unwrap();
                let payload = if case.payload.is_some() {
                    panic!()
                } else {
                    None
                };

                wasi::Value::Variant(Box::new(wasi::Variant { case_idx, payload }))
            },
            | wasi::Type::String => wasi::Value::String(u.arbitrary().unwrap()),
        }
    }

    fn guess_variable(
        &self,
        u: &mut Unstructured,
        replace: &term::Variable,
        with: Variable,
        t: &Term,
        function: &Function,
    ) -> Term {
        match t {
            | Term::Conj(conj) => {
                let mut clauses = Vec::new();

                for clause in &conj.clauses {
                    let clause = self.guess_variable(u, replace, with.clone(), clause, function);

                    match clause {
                        | Term::Value {
                            ty: _,
                            inner: wasi::Value::Bool(b),
                        } => {
                            if !b {
                                return Term::Value {
                                    ty:    wasi::Type::Bool,
                                    inner: wasi::Value::Bool(false),
                                };
                            }
                        },
                        | Term::Value { .. } => panic!("expect bool got {:?}", clause),
                        | _ => clauses.push(clause),
                    }
                }

                if clauses.iter().all(|clause| {
                    matches!(
                        clause,
                        Term::Value {
                            ty:    wasi::Type::Bool,
                            inner: wasi::Value::Bool(true),
                        }
                    )
                }) {
                    return Term::Value {
                        ty:    wasi::Type::Bool,
                        inner: wasi::Value::Bool(true),
                    };
                }

                Term::Conj(term::Conj { clauses })
            },
            | Term::Disj(disj) => {
                // Shuffle the clauses.

                let mut shuffled_clauses = disj.clauses.clone();
                let mut to_permute = shuffled_clauses.as_mut_slice();

                while to_permute.len() > 1 {
                    let idx = u.choose_index(to_permute.len()).unwrap();

                    to_permute.swap(0, idx);
                    to_permute = &mut to_permute[1..];
                }

                // The clauses are shuffled.

                let mut clauses = Vec::new();

                for clause in &shuffled_clauses {
                    let clause = self.guess_variable(u, replace, with.clone(), clause, function);

                    match clause {
                        | Term::Value {
                            ty: _,
                            inner: wasi::Value::Bool(b),
                        } => {
                            if b {
                                return Term::Value {
                                    ty:    wasi::Type::Bool,
                                    inner: wasi::Value::Bool(true),
                                };
                            }
                        },
                        | Term::Value { .. } => panic!("expect bool got {:?}", clause),
                        | _ => clauses.push(clause),
                    }
                }

                if clauses.iter().all(|clause| {
                    matches!(
                        clause,
                        Term::Value {
                            ty:    _,
                            inner: wasi::Value::Bool(false),
                        }
                    )
                }) {
                    return Term::Value {
                        ty:    wasi::Type::Bool,
                        inner: wasi::Value::Bool(false),
                    };
                }

                Term::Disj(term::Disj { clauses })
            },
            | Term::Attr(attr) => {
                match (replace, with) {
                    | (term::Variable::Attr(a), Variable::Resource(resource)) => {
                        if a == attr {
                            let param = function.params.iter().find(|p| p.name == a.param).unwrap();
                            let resource_type = self
                                .resource_types
                                .get(&Idx::Numeric(param.resource_type_idx))
                                .unwrap();

                            return Term::Value {
                                ty:    resource_type.wasi_type.clone(),
                                inner: resource.attrs.get(&a.name).unwrap().to_owned(),
                            };
                        }
                    },
                    | _ => return t.to_owned(),
                }

                return t.to_owned();
            },
            | Term::Param(param) => {
                let function_param = function
                    .params
                    .iter()
                    .find(|p| p.name == param.name)
                    .unwrap();
                let resource_type = self
                    .resource_types
                    .get(&Idx::Numeric(function_param.resource_type_idx))
                    .unwrap();
                let ty = resource_type.wasi_type.clone();

                match (replace, &with) {
                    | (term::Variable::Param(p), Variable::Resource(resource)) => {
                        if p == param {
                            return Term::Value {
                                ty,
                                inner: resource.value.clone(),
                            };
                        }
                    },
                    | (term::Variable::Param(p), Variable::Value(value)) => {
                        if p == param {
                            return Term::Value {
                                ty,
                                inner: value.to_owned(),
                            };
                        }
                    },
                    | (term::Variable::Attr(attr), Variable::Resource(resource)) => {
                        return Term::Value {
                            ty,
                            inner: resource.attrs.get(&attr.name).unwrap().clone(),
                        };
                    },
                    | _ => panic!("{:?} {:?}", replace, with),
                }

                return t.to_owned();
            },
            | Term::Value { .. } => t.to_owned(),
            | Term::ValueEq(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs, function);
                let rhs = self.guess_variable(u, replace, with, &op.rhs, function);

                match (&lhs, &rhs) {
                    | (Term::Value { ty: _, inner: l }, Term::Value { ty: _, inner: r }) => {
                        Term::Value {
                            ty:    wasi::Type::Bool,
                            inner: wasi::Value::Bool(l == r),
                        }
                    },
                    | _ => t.to_owned(),
                }
            },
            | Term::FlagUnset(op) => {
                let target = self.guess_variable(u, replace, with, &op.target, function);

                match target {
                    | Term::Value {
                        ty: wasi::Type::Flags(flags_type),
                        inner: wasi::Value::Flags(flags),
                    } => Term::Value {
                        ty:    wasi::Type::Bool,
                        inner: wasi::Value::Bool(
                            !flags
                                .fields
                                .get(
                                    flags_type
                                        .fields
                                        .iter()
                                        .enumerate()
                                        .find(|(_i, field)| *field == &op.flag)
                                        .unwrap()
                                        .0,
                                )
                                .unwrap(),
                        ),
                    },
                    | _ => panic!(),
                }
            },
            | Term::I64Add(op) => {
                let l = self.guess_variable(u, replace, with.clone(), &op.l, function);
                let r = self.guess_variable(u, replace, with, &op.r, function);

                match (&l, &r) {
                    | (
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(l),
                        },
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(r),
                        },
                    ) => Term::Value {
                        ty:    wasi::Type::S64,
                        inner: wasi::Value::S64(l.checked_add(r).unwrap()),
                    },
                    | (Term::Value { .. }, Term::Value { .. }) => {
                        panic!("expect i64, got {:?} and {:?}", l, r)
                    },
                    | _ => t.to_owned(),
                }
            },
            | Term::I64Ge(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs, function);
                let rhs = self.guess_variable(u, replace, with, &op.rhs, function);

                match (&lhs, &rhs) {
                    | (
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(lhs),
                        },
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(rhs),
                        },
                    ) => Term::Value {
                        ty:    wasi::Type::Bool,
                        inner: wasi::Value::Bool(lhs >= rhs),
                    },
                    | (Term::Value { .. }, Term::Value { .. }) => {
                        panic!("expect i64, got {:?} and {:?}", lhs, rhs)
                    },
                    | _ => t.to_owned(),
                }
            },
            | Term::I64Le(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs, function);
                let rhs = self.guess_variable(u, replace, with, &op.rhs, function);

                match (&lhs, &rhs) {
                    | (
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(lhs),
                        },
                        &Term::Value {
                            ty: _,
                            inner: wasi::Value::S64(rhs),
                        },
                    ) => Term::Value {
                        ty:    wasi::Type::Bool,
                        inner: wasi::Value::Bool(lhs <= rhs),
                    },
                    | (Term::Value { .. }, Term::Value { .. }) => {
                        panic!("expect i64, got {:?} and {:?}", lhs, rhs)
                    },
                    | _ => t.to_owned(),
                }
            },
        }
    }

    fn free_variable(&self, term: &Term) -> Option<term::Variable> {
        match term {
            | Term::Conj(conj) => {
                let mut var = None;

                for clause in &conj.clauses {
                    var = self.free_variable(clause);

                    if var.is_some() {
                        break;
                    }
                }

                var
            },
            | Term::Disj(disj) => {
                let mut var = None;

                for clause in &disj.clauses {
                    var = self.free_variable(clause);

                    if var.is_some() {
                        break;
                    }
                }

                var
            },
            | Term::Attr(attr) => Some(term::Variable::Attr(attr.to_owned())),
            | Term::Param(param) => Some(term::Variable::Param(param.to_owned())),
            | Term::Value { .. } => None,
            | Term::ValueEq(op) => {
                if let Some(var) = self.free_variable(&op.lhs) {
                    return Some(var);
                }

                self.free_variable(&op.rhs)
            },
            | Term::FlagUnset(op) => self.free_variable(&op.target),
            | Term::I64Add(op) => {
                if let Some(var) = self.free_variable(&op.l) {
                    return Some(var);
                }

                self.free_variable(&op.r)
            },
            | Term::I64Ge(op) => {
                if let Some(var) = self.free_variable(&op.lhs) {
                    return Some(var);
                }

                self.free_variable(&op.rhs)
            },
            | Term::I64Le(op) => {
                if let Some(var) = self.free_variable(&op.lhs) {
                    return Some(var);
                }

                self.free_variable(&op.rhs)
            },
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Unknown(#[from] eyre::Error),

    #[error("no solution found for input contract for function: {function}")]
    NoSolution { function: String },
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Solution {
    params: Vec<Param>,
}

impl Solution {
    pub fn new(params: Vec<Param>) -> Self {
        Self { params }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Param {
    name:  String,
    inner: ParamInner,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ParamInner {
    Resource(usize),
    Value(wasi::Value),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{resource_ctx::ResourceContext, term, wasi};

    #[test]
    fn simple_clause() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let filedelta_idx = env.resource_types_mut().push(
            Some("filedelta".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::S64,
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "offset".to_owned(),
                    resource_type_idx: filedelta_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let filedelta_resource = ctx.push(
            filedelta_idx,
            Resource {
                value: wasi::Value::S64(0),
                attrs: Default::default(),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::I64Ge(Box::new(term::I64Ge {
                    lhs: Term::Param(term::Param {
                        name: "offset".to_owned(),
                    }),
                    rhs: Term::Value {
                        ty:    wasi::Type::S64,
                        inner: wasi::Value::S64(0),
                    },
                })),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param {
                name:  "offset".to_owned(),
                inner: ParamInner::Resource(filedelta_resource),
            }],
        );
    }

    #[test]
    fn simple_clause_no_resource() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let filedelta_idx = env.resource_types_mut().push(
            Some("filedelta".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::S64,
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "offset".to_owned(),
                    resource_type_idx: filedelta_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let solution = env
            .solve(
                &mut u,
                &Term::I64Ge(Box::new(term::I64Ge {
                    lhs: Term::Param(term::Param {
                        name: "offset".to_owned(),
                    }),
                    rhs: Term::Value {
                        ty:    wasi::Type::S64,
                        inner: wasi::Value::S64(3),
                    },
                })),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param {
                name:  "offset".to_owned(),
                inner: ParamInner::Value(wasi::Value::S64(3)),
            }]
        );
    }

    #[test]
    fn simple_clause_attribute() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(10))]),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::I64Ge(Box::new(term::I64Ge {
                    lhs: Term::Attr(term::Attr {
                        param: "fd".to_owned(),
                        name:  "offset".to_owned(),
                    }),
                    rhs: Term::Value {
                        ty:    wasi::Type::S64,
                        inner: wasi::Value::S64(10),
                    },
                })),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param {
                name:  "fd".to_owned(),
                inner: ParamInner::Resource(fd_resource),
            }]
        );
    }

    #[test]
    fn conjunction() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(10))]),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::Conj(term::Conj {
                    clauses: vec![
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value {
                                ty:    wasi::Type::S64,
                                inner: wasi::Value::S64(10),
                            },
                        })),
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value {
                                ty:    wasi::Type::S64,
                                inner: wasi::Value::S64(0),
                            },
                        })),
                    ],
                }),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param {
                name:  "fd".to_owned(),
                inner: ParamInner::Resource(fd_resource),
            }]
        );
    }

    #[test]
    fn conjunction_no_solution() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let _fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(9))]),
            },
        );
        let maybe_solution = env.solve(
            &mut u,
            &Term::Conj(term::Conj {
                clauses: vec![
                    Term::I64Ge(Box::new(term::I64Ge {
                        lhs: Term::Attr(term::Attr {
                            param: "fd".to_owned(),
                            name:  "offset".to_owned(),
                        }),
                        rhs: Term::Value {
                            ty:    wasi::Type::S64,
                            inner: wasi::Value::S64(10),
                        },
                    })),
                    Term::I64Ge(Box::new(term::I64Ge {
                        lhs: Term::Attr(term::Attr {
                            param: "fd".to_owned(),
                            name:  "offset".to_owned(),
                        }),
                        rhs: Term::Value {
                            ty:    wasi::Type::S64,
                            inner: wasi::Value::S64(0),
                        },
                    })),
                ],
            }),
            &ctx,
            &Idx::Symbolic("fd_seek".to_owned()),
        );

        assert!(maybe_solution.is_none());
    }

    #[test]
    fn disjunction() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(10))]),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::Disj(term::Disj {
                    clauses: vec![
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value {
                                ty:    wasi::Type::S64,
                                inner: wasi::Value::S64(10),
                            },
                        })),
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value {
                                ty:    wasi::Type::S64,
                                inner: wasi::Value::S64(20),
                            },
                        })),
                    ],
                }),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param {
                name:  "fd".to_owned(),
                inner: ParamInner::Resource(fd_resource),
            }]
        );
    }

    #[test]
    fn nested_disj_conj() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );
        let whence_wasi_type = wasi::Type::Variant(wasi::VariantType {
            cases: vec![
                wasi::CaseType {
                    name:    "set".to_owned(),
                    payload: None,
                },
                wasi::CaseType {
                    name:    "cur".to_owned(),
                    payload: None,
                },
                wasi::CaseType {
                    name:    "end".to_owned(),
                    payload: None,
                },
            ],
        });
        let whence_idx = env.resource_types_mut().push(
            Some("whence".to_owned()),
            ResourceType {
                wasi_type:  whence_wasi_type.clone(),
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![
                    FunctionParam {
                        name:              "fd".to_owned(),
                        resource_type_idx: fd_idx,
                    },
                    FunctionParam {
                        name:              "whence".to_owned(),
                        resource_type_idx: whence_idx,
                    },
                ],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let _fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(10))]),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::Disj(term::Disj {
                    clauses: vec![
                        Term::Conj(term::Conj {
                            clauses: vec![
                                Term::ValueEq(Box::new(term::ValueEq {
                                    lhs: Term::Param(term::Param {
                                        name: "whence".to_owned(),
                                    }),
                                    rhs: Term::Value {
                                        ty:    whence_wasi_type.clone(),
                                        inner: wasi::Value::Variant(Box::new(wasi::Variant {
                                            case_idx: 0,
                                            payload:  None,
                                        })),
                                    },
                                })),
                                Term::I64Ge(Box::new(term::I64Ge {
                                    lhs: Term::Attr(term::Attr {
                                        param: "fd".to_owned(),
                                        name:  "offset".to_owned(),
                                    }),
                                    rhs: Term::Value {
                                        ty:    wasi::Type::S64,
                                        inner: wasi::Value::S64(11),
                                    },
                                })),
                            ],
                        }),
                        Term::Conj(term::Conj {
                            clauses: vec![Term::ValueEq(Box::new(term::ValueEq {
                                lhs: Term::Param(term::Param {
                                    name: "whence".to_owned(),
                                }),
                                rhs: Term::Value {
                                    ty:    whence_wasi_type,
                                    inner: wasi::Value::Variant(Box::new(wasi::Variant {
                                        case_idx: 1,
                                        payload:  None,
                                    })),
                                },
                            }))],
                        }),
                    ],
                }),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![
                Param {
                    name:  "fd".to_owned(),
                    inner: ParamInner::Value(wasi::Value::Handle(0)),
                },
                Param {
                    name:  "whence".to_owned(),
                    inner: ParamInner::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                        case_idx: 1,
                        payload:  None,
                    }))),
                }
            ]
        );
    }

    #[test]
    fn nested_disj_conj_arith() {
        let mut env = Environment::new();
        let mut ctx = ResourceContext::new();
        let fd_idx = env.resource_types_mut().push(
            Some("fd".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );
        let whence_wasi_type = wasi::Type::Variant(wasi::VariantType {
            cases: vec![
                wasi::CaseType {
                    name:    "set".to_owned(),
                    payload: None,
                },
                wasi::CaseType {
                    name:    "cur".to_owned(),
                    payload: None,
                },
                wasi::CaseType {
                    name:    "end".to_owned(),
                    payload: None,
                },
            ],
        });
        let whence_idx = env.resource_types_mut().push(
            Some("whence".to_owned()),
            ResourceType {
                wasi_type:  whence_wasi_type.clone(),
                attributes: Default::default(),
                fungible:   true,
            },
        );
        let filedelta_idx = env.resource_types_mut().push(
            Some("filedelta".to_owned()),
            ResourceType {
                wasi_type:  wasi::Type::S64,
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            Some("fd_seek".to_owned()),
            Function {
                params:         vec![
                    FunctionParam {
                        name:              "fd".to_owned(),
                        resource_type_idx: fd_idx,
                    },
                    FunctionParam {
                        name:              "offset".to_owned(),
                        resource_type_idx: filedelta_idx,
                    },
                    FunctionParam {
                        name:              "whence".to_owned(),
                        resource_type_idx: whence_idx,
                    },
                ],
                input_contract: Term::Value {
                    ty:    wasi::Type::Bool,
                    inner: wasi::Value::Bool(true),
                },
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = ctx.push(
            fd_idx,
            Resource {
                value: wasi::Value::S64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::S64(10))]),
            },
        );
        let solution = env
            .solve(
                &mut u,
                &Term::Disj(term::Disj {
                    clauses: vec![Term::Conj(term::Conj {
                        clauses: vec![
                            Term::ValueEq(Box::new(term::ValueEq {
                                lhs: Term::Param(term::Param {
                                    name: "whence".to_owned(),
                                }),
                                rhs: Term::Value {
                                    ty:    whence_wasi_type,
                                    inner: wasi::Value::Variant(Box::new(wasi::Variant {
                                        case_idx: 1,
                                        payload:  None,
                                    })),
                                },
                            })),
                            Term::I64Ge(Box::new(term::I64Ge {
                                lhs: Term::I64Add(Box::new(term::I64Add {
                                    l: Term::Attr(term::Attr {
                                        param: "fd".to_owned(),
                                        name:  "offset".to_owned(),
                                    }),
                                    r: Term::Param(term::Param {
                                        name: "offset".to_owned(),
                                    }),
                                })),
                                rhs: Term::Value {
                                    ty:    wasi::Type::S64,
                                    inner: wasi::Value::S64(10),
                                },
                            })),
                        ],
                    })],
                }),
                &ctx,
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![
                Param {
                    name:  "fd".to_owned(),
                    inner: ParamInner::Resource(fd_resource),
                },
                Param {
                    name:  "offset".to_owned(),
                    inner: ParamInner::Value(wasi::Value::S64(0)),
                },
                Param {
                    name:  "whence".to_owned(),
                    inner: ParamInner::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                        case_idx: 1,
                        payload:  None,
                    }))),
                },
            ]
        );
    }
}
