mod common;

use crate::common::run_seed;

#[test]
fn args() {
    let run = run_seed("02-args.json");
    let call = run
        .prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(call.errno, Some(0));
}
