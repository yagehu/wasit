mod common;

use crate::common::run_seed;

#[test]
fn creat() {
    let run = run_seed("00-creat.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
    assert!(run.base_dir.path().join("a").exists());
}
