#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Term {
    Not(Box<Not>),
    And(And),
    Or(Or),

    Param(Param),

    FlagsGet(Box<FlagsGet>),
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
pub struct Param {
    pub name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsGet {
    pub target: Term,
    pub field:  String,
}
