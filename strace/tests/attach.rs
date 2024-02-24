use std::{fs, process, thread, time::Duration};

use strace::{parse, Strace};
use tempfile::tempdir;

#[test]
fn attach() {
    let mut child = process::Command::new("sleep").arg("1").spawn().unwrap();
    let dir = tempdir().unwrap();
    let output = dir.path().join("out");
    let strace = Strace::attach(child.id(), &output).unwrap();

    thread::sleep(Duration::from_millis(100));

    strace.stop().unwrap();
    child.kill().unwrap();

    assert!(output.exists());

    let strace_content = fs::read_to_string(output).unwrap();
    let (_, _trace) = parse::trace(&strace_content).unwrap();
}
