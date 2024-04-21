pub mod program;

pub use program::Program;

use interval::IntervalSet;

use crate::program::ConstraintSet;

pub fn evaluate(prog: &Program) -> ConstraintSet {
    let cset = prog.expr.eval();

    cset
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Constraint {
    intervals: IntervalSet<usize>,
}

#[cfg(test)]
mod tests {
    use interval::interval_set::ToIntervalSet;
    use num_bigint::BigInt;

    use crate::{
        evaluate,
        program::{And, Expr, If, TypeRef, U64Gt, U64Lt, Unspecified},
        Program,
    };

    #[test]
    fn eval() {
        let p = Program {
            expr: Expr::If(Box::new(If {
                cond: Expr::And(Box::new(And {
                    lhs: Expr::U64Gt(Box::new(U64Gt {
                        lhs: Expr::TypeRef(TypeRef::Param {
                            name: "offset".to_owned(),
                        }),
                        rhs: Expr::Number(BigInt::from(1000)),
                    })),
                    rhs: Expr::U64Lt(Box::new(U64Lt {
                        lhs: Expr::TypeRef(TypeRef::Param {
                            name: "offset".to_owned(),
                        }),
                        rhs: Expr::Number(BigInt::from(1)),
                    })),
                })),
                then: Unspecified {
                    tref: TypeRef::Result {
                        name: "error".to_owned(),
                    },
                },
            })),
        };

        let cset = evaluate(&p);
        let iset = cset.get::<u64>(&TypeRef::Param {
            name: "offset".to_owned(),
        });

        assert_eq!(iset, vec![(1, 1000)].to_interval_set());
    }
}
