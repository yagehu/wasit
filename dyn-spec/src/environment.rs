use std::collections::{BTreeSet, HashMap};

use arbitrary::Unstructured;

use crate::{
    ast::Idx,
    term::{self, Variable},
    wasi,
    IndexSpace,
    Term,
};

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
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub value: wasi::Value,
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

        self.solve_helper(u, t, function, &mut solution);

        if solution.is_empty() {
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
            | Variable::Attr(attr) => &attr.param,
            | Variable::Param(param) => &param.name,
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

        if resource_idxs.is_empty() {
            return false;
        }

        // Shuffle the resource pool.

        let mut to_permute = resource_idxs.as_mut_slice();

        while to_permute.len() > 1 {
            let idx = u.choose_index(to_permute.len()).unwrap();

            to_permute.swap(0, idx);
            to_permute = &mut to_permute[1..];
        }

        // `resource_idxs` is shuffled.

        for resource_idx in resource_idxs {
            let resource = self.resources.get(resource_idx).unwrap();

            solution.push(Param::Resource(resource_idx));

            let guess = self.guess_variable(&var, resource.value.clone(), t);
            let solved = self.solve_helper(u, &guess, function_idx, solution);

            if solved {
                break;
            }

            solution.pop();
        }

        false
    }

    fn guess_variable(&self, variable: &Variable, value: wasi::Value, t: &Term) -> Term {
        match t {
            | Term::Conj(conj) => {
                let mut clauses = Vec::new();

                for clause in &conj.clauses {
                    let clause = self.guess_variable(variable, value.clone(), clause);

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
            | Term::Disj(disj) => todo!(),
            | Term::Attr(attr) => {
                if let Variable::Attr(a) = variable {
                    if a == attr {
                        return Term::Value(value);
                    }
                }

                return t.to_owned();
            },
            | Term::Param(param) => {
                if let Variable::Param(p) = variable {
                    if p == param {
                        return Term::Value(value);
                    }
                }

                return t.to_owned();
            },
            | Term::Value(_v) => t.to_owned(),
            | Term::I64Ge(op) => {
                let lhs = self.guess_variable(variable, value.clone(), &op.lhs);
                let rhs = self.guess_variable(variable, value, &op.rhs);

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
        }
    }

    fn free_variable(&self, term: &Term) -> Option<Variable> {
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
            | Term::Attr(attr) => Some(Variable::Attr(attr.to_owned())),
            | Term::Param(param) => Some(Variable::Param(param.to_owned())),
            | Term::Value(_) => None,
            | Term::I64Ge(op) => {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term;

    #[test]
    fn simple_clause() {
        let mut env = Environment::new();
        let filedelta_idx = env.resource_types_mut().push(
            "filedelta".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::I64,
                attributes: Default::default(),
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
    fn simple_clause_no_solution() {
        let mut env = Environment::new();
        let filedelta_idx = env.resource_types_mut().push(
            "filedelta".to_owned(),
            ResourceType {
                wasi_type:  wasi::Type::I64,
                attributes: Default::default(),
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
        let maybe_solution = env.solve(
            &mut u,
            &Term::I64Ge(Box::new(term::I64Ge {
                lhs: Term::Param(term::Param {
                    name: "offset".to_owned(),
                }),
                rhs: Term::Value(wasi::Value::I64(0)),
            })),
            &Idx::Symbolic("fd_seek".to_owned()),
        );

        assert!(maybe_solution.is_none(), "{:?}", maybe_solution);
    }
}
