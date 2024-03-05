mod common;

use crate::common::run_seed;

#[test]
fn clock() {
    let run = run_seed("04-clock.json");
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
