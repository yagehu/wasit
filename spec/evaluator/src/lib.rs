pub mod program;

pub use program::Program;

use interval::IntervalSet;

use crate::program::{ConstraintSet, TypeRef};

pub fn evaluate(ctx: &Context, prog: &Program) -> ConstraintSet {
    let cset = prog.expr.eval();

    cset
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Context {}

impl Context {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Constraint {
    intervals: IntervalSet<usize>,
}

#[cfg(test)]
mod tests {
    use arbitrary::Unstructured;
    use gcollections::ops::{Bounded, Cardinality};
    use interval::interval_set::ToIntervalSet;
    use num_bigint::BigInt;
    use rand::{thread_rng, RngCore};

    use crate::{
        evaluate,
        program::{And, Expr, If, TypeRef, U64Gt, U64Lt, Unspecified},
        Context,
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

        let ctx = Context::new();
        let cset = evaluate(&ctx, &p);
        let iset = cset.get::<u64>(&TypeRef::Param {
            name: "offset".to_owned(),
        });

        assert_eq!(iset, vec![(1, 1000)].to_interval_set());
    }

    #[test]
    fn ok() {
        let mut rng = thread_rng();
        let mut data = vec![0u8; 64];

        rng.fill_bytes(&mut data);

        let iset0 = vec![(1u64, 1), (10, 19), (30, 100)].to_interval_set();
        let iset1 = vec![(1, 10), (10, 20)].to_interval_set();

        assert_eq!(iset0.interval_count(), 3);
        assert_eq!(iset1.interval_count(), 1);

        let mut u = Unstructured::new(&data);
        let is0 = iset0.iter().collect::<Vec<_>>();
        let size: usize = is0.into_iter().map(|int| int.size() as usize).sum();

        eprintln!("{size}");

        for _ in 0..30 {
            let mut i = u.choose_index(size).unwrap() as u64;
            let mut value = 0;

            for int in iset0.iter() {
                let size = int.size();

                if size > i {
                    value = int.lower() + i;
                    break;
                }

                i -= size;
            }

            eprintln!("value {value}");
        }
    }
}
