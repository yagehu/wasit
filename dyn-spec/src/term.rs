use crate::wasi;

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
