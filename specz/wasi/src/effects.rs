use crate::WasiValue;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Stmt {
    AttrSet(AttrSet),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AttrSet {
    pub resource: String,
    pub attr:     String,
    pub value:    Expr,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Expr {
    WasiValue(WasiValue),
}
