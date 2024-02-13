mod common;

use crate::common::run_seed;
use std::fmt::Pointer;

#[test]
fn environ() {
    let run = run_seed("03-environ.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
}
