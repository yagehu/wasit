use std::collections::{BTreeSet, HashMap};

use arbitrary::Unstructured;

use crate::{ast::Idx, term, wasi, IndexSpace, Term};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Variable {
    Resource(Resource),
    Value(wasi::Value),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub params: Vec<FunctionParam>,
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
pub struct Resource {
    pub value: wasi::Value,
    pub attrs: HashMap<String, wasi::Value>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Environment {
    functions:      IndexSpace<Function>,
    resource_types: IndexSpace<ResourceType>,

    resources:          Vec<Resource>,
    resources_by_types: HashMap<usize, BTreeSet<usize>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            functions:      Default::default(),
            resource_types: Default::default(),

            resources:          Default::default(),
            resources_by_types: Default::default(),
        }
    }

    pub fn insert_resource(&mut self, resource_type: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(
                self.resource_types
                    .resolve_idx(&Idx::Symbolic(resource_type))
                    .unwrap(),
            )
            .or_default()
            .insert(self.resources.len() - 1);

        self.resources.len() - 1
    }

    pub fn functions_mut(&mut self) -> &mut IndexSpace<Function> {
        &mut self.functions
    }

    pub fn resource_types_mut(&mut self) -> &mut IndexSpace<ResourceType> {
        &mut self.resource_types
    }

    pub fn solve(&self, u: &mut Unstructured, t: &Term, function: &Idx) -> Option<Solution> {
        let mut solution = Vec::new();

        if !self.solve_helper(u, t, function, &mut solution) {
            return None;
        }

        Some(Solution::new(solution))
    }

    fn solve_helper(
        &self,
        u: &mut Unstructured,
        t: &Term,
        function_idx: &Idx,
        solution: &mut Vec<Param>,
    ) -> bool {
        let var = match self.free_variable(t) {
            | Some(var) => var,
            | None => match t {
                | &Term::Value(wasi::Value::Bool(b)) => return b,
                | Term::Value(_) => panic!("terminal has wrong type {:?}", t),
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
        let mut resource_idxs = self
            .resources_by_types
            .get(&param.resource_type_idx)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();

        // Shuffle the resource pool.

        let mut to_permute = resource_idxs.as_mut_slice();

        while to_permute.len() > 1 {
            let idx = u.choose_index(to_permute.len()).unwrap();

            to_permute.swap(0, idx);
            to_permute = &mut to_permute[1..];
        }

        // `resource_idxs` is shuffled.

        let mut i = 0;
        let mut value = self.generate_value(u, &resource_type.wasi_type);
        let search_start = value.clone();

        loop {
            if !resource_idxs.is_empty() && i < resource_idxs.len() && u.ratio(95, 100).unwrap() {
                // Most of the time, try the resource.

                let resource_idx = *resource_idxs.get(i).unwrap();
                let resource = self.resources.get(resource_idx).unwrap();

                solution.push(Param::Resource(resource_idx));

                let guess = self.guess_variable(u, &var, Variable::Resource(resource.clone()), t);
                let solved = self.solve_helper(u, &guess, function_idx, solution);

                if solved {
                    return true;
                }

                i += 1;
                solution.pop();
            } else {
                if !resource_type.fungible {
                    break;
                }

                solution.push(Param::Value(value.clone()));

                let guess = self.guess_variable(u, &var, Variable::Value(value.clone()), t);
                let solved = self.solve_helper(u, &guess, function_idx, solution);

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
                            case_name: variant_type.cases.get(case_idx).unwrap().name.clone(),
                            payload: None,
                        }))
                    },
                    | (wasi::Value::I64(i), _) => wasi::Value::I64(i.wrapping_add(1)),
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
            | wasi::Type::I64 => wasi::Value::I64(u.arbitrary().unwrap()),
            | wasi::Type::U32 => todo!(),
            | wasi::Type::U64 => todo!(),
            | wasi::Type::Handle => wasi::Value::Handle(u.arbitrary().unwrap()),
            | wasi::Type::Variant(variant) => {
                let cases = variant.cases.iter().enumerate().collect::<Vec<_>>();
                let &(case_idx, case) = u.choose(&cases).unwrap();
                let payload = if case.payload.is_some() {
                    panic!()
                } else {
                    None
                };

                wasi::Value::Variant(Box::new(wasi::Variant {
                    case_idx,
                    case_name: case.name.clone(),
                    payload,
                }))
            },
        }
    }

    fn guess_variable(
        &self,
        u: &mut Unstructured,
        replace: &term::Variable,
        with: Variable,
        t: &Term,
    ) -> Term {
        match t {
            | Term::Conj(conj) => {
                let mut clauses = Vec::new();

                for clause in &conj.clauses {
                    let clause = self.guess_variable(u, replace, with.clone(), clause);

                    match clause {
                        | Term::Value(wasi::Value::Bool(b)) => {
                            if !b {
                                return Term::Value(wasi::Value::Bool(false));
                            }
                        },
                        | Term::Value(_) => panic!("expect bool got {:?}", clause),
                        | _ => clauses.push(clause),
                    }
                }

                if clauses
                    .iter()
                    .all(|clause| matches!(clause, Term::Value(wasi::Value::Bool(true))))
                {
                    return Term::Value(wasi::Value::Bool(true));
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
                    let clause = self.guess_variable(u, replace, with.clone(), clause);

                    match clause {
                        | Term::Value(wasi::Value::Bool(b)) => {
                            if b {
                                return Term::Value(wasi::Value::Bool(true));
                            }
                        },
                        | Term::Value(_) => panic!("expect bool got {:?}", clause),
                        | _ => clauses.push(clause),
                    }
                }

                if clauses
                    .iter()
                    .all(|clause| matches!(clause, Term::Value(wasi::Value::Bool(false))))
                {
                    return Term::Value(wasi::Value::Bool(false));
                }

                Term::Disj(term::Disj { clauses })
            },
            | Term::Attr(attr) => {
                match (replace, with) {
                    | (term::Variable::Attr(a), Variable::Resource(resource)) => {
                        if a == attr {
                            return Term::Value(resource.attrs.get(&a.name).unwrap().to_owned());
                        }
                    },
                    | _ => return t.to_owned(),
                }

                return t.to_owned();
            },
            | Term::Param(param) => {
                match (replace, &with) {
                    | (term::Variable::Param(p), Variable::Resource(resource)) => {
                        if p == param {
                            return Term::Value(resource.value.clone());
                        }
                    },
                    | (term::Variable::Param(p), Variable::Value(value)) => {
                        if p == param {
                            return Term::Value(value.to_owned());
                        }
                    },
                    | (term::Variable::Attr(attr), Variable::Resource(resource)) => {
                        return Term::Value(resource.attrs.get(&attr.name).unwrap().clone());
                    },
                    | _ => panic!("{:?} {:?}", replace, with),
                }

                return t.to_owned();
            },
            | Term::Value(_v) => t.to_owned(),
            | Term::ValueEq(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs);
                let rhs = self.guess_variable(u, replace, with, &op.rhs);

                match (&lhs, &rhs) {
                    | (Term::Value(l), Term::Value(r)) => Term::Value(wasi::Value::Bool(l == r)),
                    | _ => t.to_owned(),
                }
            },
            | Term::I64Add(op) => {
                let l = self.guess_variable(u, replace, with.clone(), &op.l);
                let r = self.guess_variable(u, replace, with, &op.r);

                match (&l, &r) {
                    | (&Term::Value(wasi::Value::I64(l)), &Term::Value(wasi::Value::I64(r))) => {
                        Term::Value(wasi::Value::I64(l.checked_add(r).unwrap()))
                    },
                    | (Term::Value(_), Term::Value(_)) => {
                        panic!("expect i64, got {:?} and {:?}", l, r)
                    },
                    | _ => t.to_owned(),
                }
            },
            | Term::I64Ge(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs);
                let rhs = self.guess_variable(u, replace, with, &op.rhs);

                match (&lhs, &rhs) {
                    | (
                        &Term::Value(wasi::Value::I64(lhs)),
                        &Term::Value(wasi::Value::I64(rhs)),
                    ) => Term::Value(wasi::Value::Bool(lhs >= rhs)),
                    | (Term::Value(_), Term::Value(_)) => {
                        panic!("expect i64, got {:?} and {:?}", lhs, rhs)
                    },
                    | _ => t.to_owned(),
                }
            },
            | Term::I64Le(op) => {
                let lhs = self.guess_variable(u, replace, with.clone(), &op.lhs);
                let rhs = self.guess_variable(u, replace, with, &op.rhs);

                match (&lhs, &rhs) {
                    | (
                        &Term::Value(wasi::Value::I64(lhs)),
                        &Term::Value(wasi::Value::I64(rhs)),
                    ) => Term::Value(wasi::Value::Bool(lhs <= rhs)),
                    | (Term::Value(_), Term::Value(_)) => {
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
            | Term::Value(_) => None,
            | Term::ValueEq(op) => {
                if let Some(var) = self.free_variable(&op.lhs) {
                    return Some(var);
                }

                self.free_variable(&op.rhs)
            },
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
pub enum Param {
    Resource(usize),
    Value(wasi::Value),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{term, wasi};

    #[test]
    fn simple_clause() {
        let mut env = Environment::new();
        let filedelta_idx = env.resource_types_mut().push(
            "filedelta".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::I64,
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "offset".to_owned(),
                    resource_type_idx: filedelta_idx,
                }],
            },
        );

        let mut u = Unstructured::new(&[]);
        let filedelta_resource = env.insert_resource(
            "filedelta".to_owned(),
            Resource {
                value: wasi::Value::I64(0),
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
                    rhs: Term::Value(wasi::Value::I64(0)),
                })),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(solution.params, vec![Param::Resource(filedelta_resource)]);
    }

    #[test]
    fn simple_clause_no_resource() {
        let mut env = Environment::new();
        let filedelta_idx = env.resource_types_mut().push(
            "filedelta".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::I64,
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "offset".to_owned(),
                    resource_type_idx: filedelta_idx,
                }],
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
                    rhs: Term::Value(wasi::Value::I64(3)),
                })),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(solution.params, vec![Param::Value(wasi::Value::I64(3))]);
    }

    #[test]
    fn simple_clause_attribute() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(10))]),
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
                    rhs: Term::Value(wasi::Value::I64(10)),
                })),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(solution.params, vec![Param::Resource(fd_resource)]);
    }

    #[test]
    fn conjunction() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(10))]),
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
                            rhs: Term::Value(wasi::Value::I64(10)),
                        })),
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value(wasi::Value::I64(0)),
                        })),
                    ],
                }),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(solution.params, vec![Param::Resource(fd_resource)]);
    }

    #[test]
    fn conjunction_no_solution() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
            },
        );

        let mut u = Unstructured::new(&[]);
        let _fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(9))]),
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
                        rhs: Term::Value(wasi::Value::I64(10)),
                    })),
                    Term::I64Ge(Box::new(term::I64Ge {
                        lhs: Term::Attr(term::Attr {
                            param: "fd".to_owned(),
                            name:  "offset".to_owned(),
                        }),
                        rhs: Term::Value(wasi::Value::I64(0)),
                    })),
                ],
            }),
            &Idx::Symbolic("fd_seek".to_owned()),
        );

        assert!(maybe_solution.is_none());
    }

    #[test]
    fn disjunction() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![FunctionParam {
                    name:              "fd".to_owned(),
                    resource_type_idx: fd_idx,
                }],
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(10))]),
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
                            rhs: Term::Value(wasi::Value::I64(10)),
                        })),
                        Term::I64Ge(Box::new(term::I64Ge {
                            lhs: Term::Attr(term::Attr {
                                param: "fd".to_owned(),
                                name:  "offset".to_owned(),
                            }),
                            rhs: Term::Value(wasi::Value::I64(20)),
                        })),
                    ],
                }),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(solution.params, vec![Param::Resource(fd_resource)]);
    }

    #[test]
    fn nested_disj_conj() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );
        let whence_idx = env.resource_types_mut().push(
            "whence".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Variant(wasi::VariantType {
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
                }),
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![
                    FunctionParam {
                        name:              "fd".to_owned(),
                        resource_type_idx: fd_idx,
                    },
                    FunctionParam {
                        name:              "whence".to_owned(),
                        resource_type_idx: whence_idx,
                    },
                ],
            },
        );

        let mut u = Unstructured::new(&[]);
        let _fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(10))]),
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
                                    rhs: Term::Value(wasi::Value::Variant(Box::new(
                                        wasi::Variant {
                                            case_idx:  0,
                                            case_name: "set".to_owned(),
                                            payload:   None,
                                        },
                                    ))),
                                })),
                                Term::I64Ge(Box::new(term::I64Ge {
                                    lhs: Term::Attr(term::Attr {
                                        param: "fd".to_owned(),
                                        name:  "offset".to_owned(),
                                    }),
                                    rhs: Term::Value(wasi::Value::I64(11)),
                                })),
                            ],
                        }),
                        Term::Conj(term::Conj {
                            clauses: vec![Term::ValueEq(Box::new(term::ValueEq {
                                lhs: Term::Param(term::Param {
                                    name: "whence".to_owned(),
                                }),
                                rhs: Term::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                                    case_idx:  1,
                                    case_name: "cur".to_owned(),
                                    payload:   None,
                                }))),
                            }))],
                        }),
                    ],
                }),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![Param::Value(wasi::Value::Variant(Box::new(
                wasi::Variant {
                    case_idx:  1,
                    case_name: "cur".to_owned(),
                    payload:   None,
                }
            )))]
        );
    }

    #[test]
    fn nested_disj_conj_arith() {
        let mut env = Environment::new();
        let fd_idx = env.resource_types_mut().push(
            "fd".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Handle,
                attributes: HashMap::from([("offset".to_owned(), wasi::Type::U64)]),
                fungible:   false,
            },
        );
        let whence_idx = env.resource_types_mut().push(
            "whence".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::Variant(wasi::VariantType {
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
                }),
                attributes: Default::default(),
                fungible:   true,
            },
        );

        env.functions_mut().push(
            "fd_seek".to_owned(),
            Function {
                params: vec![
                    FunctionParam {
                        name:              "fd".to_owned(),
                        resource_type_idx: fd_idx,
                    },
                    FunctionParam {
                        name:              "whence".to_owned(),
                        resource_type_idx: whence_idx,
                    },
                ],
            },
        );

        let mut u = Unstructured::new(&[]);
        let fd_resource = env.insert_resource(
            "fd".to_owned(),
            Resource {
                value: wasi::Value::I64(3),
                attrs: HashMap::from([("offset".to_owned(), wasi::Value::I64(10))]),
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
                                rhs: Term::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                                    case_idx:  1,
                                    case_name: "cur".to_owned(),
                                    payload:   None,
                                }))),
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
                                rhs: Term::Value(wasi::Value::I64(10)),
                            })),
                        ],
                    })],
                }),
                &Idx::Symbolic("fd_seek".to_owned()),
            )
            .expect("no solution found");

        assert_eq!(
            solution.params,
            vec![
                Param::Value(wasi::Value::Variant(Box::new(wasi::Variant {
                    case_idx:  1,
                    case_name: "cur".to_owned(),
                    payload:   None,
                }))),
                Param::Resource(fd_resource)
            ]
        );
    }
}
