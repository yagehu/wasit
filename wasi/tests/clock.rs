mod common;

use crate::common::run_seed;

#[test]
fn clock() {
    let run = run_seed("04-clock.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
}
