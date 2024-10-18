use z3::ast::{Ast, Int};

fn main() {
    let cfg = z3::Config::new();
    let ctx = z3::Context::new(&cfg);
    let solver = z3::Solver::new(&ctx);
    let s = z3::ast::String::new_const(&ctx, "s");

    solver.assert(&s.length()._eq(&Int::from_u64(&ctx, 0)).not());
    solver.assert(
        &s.at(&Int::from_u64(&ctx, 0))
            ._eq(&z3::ast::String::from_str(&ctx, "/").unwrap())
            .not(),
    );

    // let some_idx = Int::fresh_const(&ctx, "");

    assert_eq!(solver.check(), z3::SatResult::Sat);

    let model = solver.get_model().unwrap();

    println!("{:#?}", model);
}
