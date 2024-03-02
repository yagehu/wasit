use std::{fs, path::PathBuf};

use nom::combinator::all_consuming;
use strace::parse::{self, Trace};

#[test]
fn parse() {
    let trace_content = fs::read_to_string(PathBuf::from("testdata").join("01")).unwrap();
    let (_rest, _trace) = all_consuming(Trace::parse)(&trace_content).unwrap();
}
