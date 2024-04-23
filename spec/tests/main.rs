use std::fs;

use wazzi_spec::parsers::{wazzi_preview1, Span};

#[test]
fn preview1() {
    let s = fs::read_to_string("preview1.witx").unwrap();
    let input = Span::new(&s);
    let result = wazzi_preview1::Document::parse(input);
    let doc = match result {
        | Ok(doc) => doc,
        | Err(err) => {
            eprintln!("{err}");
            panic!();
        },
    };
    let _spec = doc.into_package().unwrap();
}
