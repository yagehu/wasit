mod common;

use std::fs;

use crate::common::run_seed;

#[test]
fn creat() {
    let run = run_seed("00-creat.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
    assert!(run.base_dir.path().join("a").exists());
}

#[test]
fn write() {
    let run = run_seed("01-write.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));

    let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

    assert_eq!(file_content, vec![97, 98]);
}
