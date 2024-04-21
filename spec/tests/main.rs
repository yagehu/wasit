use std::fs;

use wazzi_spec::{
    package::{Defvaltype, TypeidxBorrow},
    parsers::{wazzi_preview1, Span},
};

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
    let spec = doc.into_package().unwrap();
    // let interface = spec
    //     .interface(TypeidxBorrow::Symbolic("wasi_snapshot_preview1"))
    //     .unwrap();
    // let fd_type = interface
    //     .get_resource_type(TypeidxBorrow::Symbolic("fd"))
    //     .unwrap();

    // assert_eq!(fd_type, &Defvaltype::Handle);

    // let path_open = interface.function("path_open").unwrap();

    // assert_eq!(path_open.name, "path_open");
    // assert_eq!(path_open.params.len(), 7);
    // assert_eq!(path_open.results.len(), 1);
}
