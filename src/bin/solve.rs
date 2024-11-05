use z3::ast::{forall_const, Ast, Bool, Int, String};

fn main() {
    let cfg = z3::Config::new();
    let ctx = z3::Context::new(&cfg);
    let solver = z3::Solver::new(&ctx);
    let path = String::new_const(&ctx, "path");

    solver.assert(&path.length()._eq(&Int::from_u64(&ctx, 10)));

    let idx = Int::fresh_const(&ctx, "");

    solver.assert(&forall_const(
        &ctx,
        &[&idx],
        &[],
        &Bool::and(
            &ctx,
            &[&Int::from_u64(&ctx, 0).le(&idx), &idx.lt(&path.length())],
        )
        .implies(&Bool::and(
            &ctx,
            &[idx.ge(&Int::from_u64(&ctx, 1)), path.at(idx - 1)],
        )),
    ));

    assert_eq!(solver.check(), z3::SatResult::Sat);

    let model = solver.get_model();

    println!("{:#?}", model);
}
