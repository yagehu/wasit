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

    let _trace = match parse::trace(&strace_content) {
        | Ok((_rest, trace)) => trace,
        | Err(_err) => {
            // If parsing errors, discarding the final line should fix it.

            let strace_content = strace_content
                .rsplitn(2, '\n')
                .collect::<Vec<_>>()
                .get(1)
                .cloned()
                .unwrap_or_default();

            parse::trace(strace_content)
                .expect(&format!("{strace_content}"))
                .1
        },
    };
}
