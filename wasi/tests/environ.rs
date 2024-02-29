mod common;

use crate::common::run_seed;

#[test]
fn environ() {
    let run = run_seed("03-environ.json");
    let prog = run.result.expect(&run.stderr);
    let call = prog.call_store().last().unwrap().unwrap().read().unwrap();

    assert_eq!(call.errno, Some(0));
}
