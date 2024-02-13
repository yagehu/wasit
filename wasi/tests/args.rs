mod common;

use crate::common::run_seed;
use std::fmt::Pointer;

#[test]
fn args() {
    let run = run_seed("02-args.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
}
