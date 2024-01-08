use wazzi_witx::{Id, Resource};

fn document() -> wazzi_witx::Document {
    wazzi_witx::load(&[
        "../spec/preview1/witx/typenames.witx",
        "../spec/preview1/witx/wasi_snapshot_preview1.witx",
    ])
    .unwrap()
}

#[test]
fn resources_exist() {
    let doc = document();
    let cases = vec!["argv", "argv_buf", "argv_size", "argv_buf_size", "fd"];

    for case in &cases {
        let resource = doc.resource(&Id::new(case));

        assert!(matches!(resource, Some(_)), "resource `{case}` not found");
    }
}

#[test]
fn argv() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let args_get = module.func(&Id::new("args_get")).unwrap();
    let args_sizes_get = module.func(&Id::new("args_sizes_get")).unwrap();
    let args_sizes_get_results = args_sizes_get.unpack_expected_result();
    let argv_size_resource = args_sizes_get_results[0].resource(&doc).unwrap();
    let argv_buf_size_resource = args_sizes_get_results[1].resource(&doc).unwrap();
    let argv_size_can_fulfill = argv_size_resource.can_fulfill(&doc);
    let argv_buf_size_can_fulfill = argv_buf_size_resource.can_fulfill(&doc);

    assert_eq!(argv_size_can_fulfill[0].name, Id::new("argv"));
    assert_eq!(argv_size_can_fulfill[0].tref, args_get.params[0].tref);
    assert_eq!(argv_buf_size_can_fulfill[0].name, Id::new("argv_buf"));
    assert_eq!(argv_buf_size_can_fulfill[0].tref, args_get.params[1].tref);
}

#[test]
fn path_open() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let path_open = module.func(&Id::new("path_open")).unwrap();
    let fd = &path_open.params[0].tref.resource(&doc);

    assert!(fd.is_some());
}

#[test]
fn fd_write() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let fd_write = module.func(&Id::new("fd_write")).unwrap();
    let iovs = &fd_write.params[1];
    let ciovec_tref = match iovs.tref.type_().as_ref() {
        | wazzi_witx::Type::List(tref) => tref,
        | _ => panic!(),
    };

    assert!(ciovec_tref.resource(&doc).is_some());

    let ciovec_record = match ciovec_tref.type_().as_ref() {
        | wazzi_witx::Type::Record(record) => record,
        | _ => panic!(),
    };
    let buf_resource = ciovec_record.members[0].tref.resource(&doc).unwrap();
    let buf_len_resource = ciovec_record.members[1].tref.resource(&doc).unwrap();

    let buf_len_can_fulfills = buf_len_resource.can_fulfill(&doc);

    assert!(matches!(
        buf_len_can_fulfills[0],
        Resource {
            name,
            ..
        } if name == &buf_resource.name,
    ));
}
