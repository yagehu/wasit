mod common;

use crate::common::run_seed;

#[test]
fn environ() {
    let run = run_seed("03-environ.json");
    let prog = run.result.expect(&run.stderr);
    let call = prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(call.errno, Some(0));
}
