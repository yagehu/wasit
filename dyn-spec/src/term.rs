use crate::{
    environment::{Function, FunctionParam},
    wasi,
    Environment,
};

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

    I64Ge(Box<I64Ge>),
}

impl Term {
    pub fn bound_params(&self, env: &Environment, function: &Function) -> Vec<FunctionParam> {
        fn helper(
            term: &Term,
            env: &Environment,
            function: &Function,
            list: &mut Vec<FunctionParam>,
        ) {
            match term {
                | Term::Conj(conj) => {
                    for clause in &conj.clauses {
                        helper(clause, env, function, list);
                    }
                },
                | Term::Disj(disj) => {
                    for clause in &disj.clauses {
                        helper(clause, env, function, list);
                    }
                },
                | Term::Attr(attr) => {
                    let param = function
                        .params
                        .iter()
                        .find(|p| p.name == attr.param)
                        .unwrap();

                    list.push(param.to_owned());
                },
                | Term::Param(param) => list.push(
                    function
                        .params
                        .iter()
                        .find(|p| p.name == param.name)
                        .unwrap()
                        .to_owned(),
                ),
                | Term::Value(_) => return,
                | Term::I64Ge(op) => {
                    helper(&op.lhs, env, function, list);
                    helper(&op.rhs, env, function, list);
                },
            }
        }

        let mut ret = Vec::new();

        helper(self, env, function, &mut ret);

        ret
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
pub struct I64Ge {
    pub lhs: Term,
    pub rhs: Term,
}
