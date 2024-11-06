use std::ops::Sub;

use z3::ast::{forall_const, Ast, Bool, Int, String};

fn main() {
    let cfg = z3::Config::new();
    let ctx = z3::Context::new(&cfg);
    let solver = z3::Solver::new(&ctx);
    let path = String::new_const(&ctx, "path");

    let i = Int::fresh_const(&ctx, "");
    let j = Int::fresh_const(&ctx, "");

    solver.assert(&forall_const(
        &ctx,
        &[&i, &j],
        &[],
        &Bool::and(
            &ctx,
            &[
                &Int::from_u64(&ctx, 0).le(&i),
                &Int::from_u64(&ctx, 0).le(&j),
                &i.lt(&path.length()),
                &j.le(&path.length()),
                &Bool::or(
                    &ctx,
                    &[
                        &Bool::and(
                            &ctx,
                            &[
                                &i.ge(&Int::from_u64(&ctx, 1)),
                                &path
                                    .at(&i.clone().sub(Int::from_u64(&ctx, 1)))
                                    ._eq(&String::from_str(&ctx, "/").unwrap()),
                            ],
                        ),
                        &i._eq(&Int::from_u64(&ctx, 0)),
                    ],
                ),
                &Bool::or(
                    &ctx,
                    &[
                        &Bool::and(
                            &ctx,
                            &[
                                &j.lt(&path.length()),
                                &path.at(&j)._eq(&String::from_str(&ctx, "/").unwrap()),
                            ],
                        ),
                        &j._eq(&path.length()),
                    ],
                ),
                &path
                    .substr(&i, &j.clone().sub(i.clone()))
                    .contains(&String::from_str(&ctx, "/").unwrap())
                    .not(),
            ],
        )
        .implies(&Bool::and(
            &ctx,
            &[Bool::or(
                &ctx,
                &[
                    path.substr(&i, &j.clone().sub(i.clone()))
                        ._eq(&String::from_str(&ctx, "..").unwrap()),
                    path.substr(&i, &j.clone().sub(i.clone()))
                        .contains(&String::from_str(&ctx, ".").unwrap())
                        .not(),
                ],
            )],
        )),
    ));
    // solver.assert(&path.contains(&String::from_str(&ctx, "/").unwrap()));
    // solver.assert(&path.contains(&String::from_str(&ctx, ".").unwrap()));
    // solver.assert(&path.contains(&String::from_str(&ctx, "a").unwrap()));
    solver.assert(&path.length()._eq(&Int::from_u64(&ctx, 8)));
    solver.assert(&forall_const(
        &ctx,
        &[&i],
        &[],
        &Bool::and(&ctx, &[Int::from_u64(&ctx, 0).le(&i), i.lt(&path.length())]).implies(
            &Bool::or(
                &ctx,
                &[
                    path.at(&i)._eq(&String::from_str(&ctx, "/").unwrap()),
                    path.at(&i)._eq(&String::from_str(&ctx, ".").unwrap()),
                    path.at(&i)._eq(&String::from_str(&ctx, "a").unwrap()),
                ],
            ),
        ),
    ));

    for _i in 0..10 {
        assert_eq!(solver.check(), z3::SatResult::Sat);

        let model = solver.get_model().unwrap();

        println!(
            "{}",
            model
                .eval(&path, true)
                .unwrap()
                .as_string()
                .unwrap()
                .as_str()
        );

        solver.assert(&path._eq(&model.eval(&path, true).unwrap()).not());
    }
}
