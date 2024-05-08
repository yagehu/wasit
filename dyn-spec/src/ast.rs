#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Spec {
    pub exprs: Vec<Expr>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Expr {
    AttrSet(AttrSet),
    Enum(Enum),
    If(Box<If>),
    ValueEq(Box<ValueEq>),
    Param(Idx),
    Result(Idx),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Value {
    I64(i64),
    U64(u64),
    Param(Idx),
    Result(Idx),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum VarRef {
    Param(Idx),
    Result(Idx),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AttrSet {
    pub var:   VarRef,
    pub attr:  String,
    pub value: Value,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Enum {
    pub typename: Idx,
    pub variant:  Idx,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct If {
    pub cond: Expr,
    pub then: Vec<Expr>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ValueEq {
    pub lhs: Expr,
    pub rhs: Expr,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Idx {
    Symbolic(String),
    Numeric(usize),
}
