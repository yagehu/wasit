use num_bigint::BigInt;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Term {
    Not(Box<Not>),
    And(And),
    Or(Or),

    AttrGet(Box<AttrGet>),
    Param(Param),

    FlagsGet(Box<FlagsGet>),
    IntConst(BigInt),
    IntAdd(Box<IntAdd>),
    IntLe(Box<IntLe>),

    ValueEq(Box<ValueEq>),

    VariantConst(Box<VariantConst>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Not {
    pub term: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct And {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Or {
    pub clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AttrGet {
    pub target: Term,
    pub attr:   String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Param {
    pub name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsGet {
    pub target: Term,
    pub r#type: String,
    pub field:  String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IntAdd {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IntLe {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ValueEq {
    pub lhs: Term,
    pub rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantConst {
    pub ty:      String,
    pub case:    String,
    pub payload: Option<Term>,
}
