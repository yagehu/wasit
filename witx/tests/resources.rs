use wazzi_witx::{Id, ResourceRelation};

fn document() -> wazzi_witx::Document {
    wazzi_witx::load(&[
        "../spec/preview1/witx/typenames.witx",
        "../spec/preview1/witx/wasi_snapshot_preview1.witx",
    ])
    .unwrap()
}

#[test]
fn resource_relations() {
    struct Case<'a> {
        from:     &'a str,
        to:       &'a str,
        expected: ResourceRelation,
    }

    let doc = document();
    let cases = vec![
        Case {
            from:     "newfd",
            to:       "fd",
            expected: ResourceRelation::Subtype,
        },
        Case {
            from:     "argv_size",
            to:       "argv",
            expected: ResourceRelation::Alloc,
        },
    ];

    for case in cases {
        let rel = doc.resource_relation(&Id::new(case.from), &Id::new(case.to));

        assert_eq!(rel, case.expected);
    }
}

#[test]
fn resources_exist() {
    let doc = document();
    let cases = vec![
        "argv",
        "argv_buf",
        "argv_size",
        "argv_buf_size",
        "environ",
        "environ_size",
        "environ_buf",
        "environ_buf_size",
        "fd",
        "newfd",
    ];

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

    assert!(argv_size_can_fulfill
        .iter()
        .any(|resource| resource.name.as_str() == "argv"));
    assert!(argv_size_can_fulfill
        .iter()
        .any(|resource| resource.tref == args_get.params[0].tref));
    assert!(argv_buf_size_can_fulfill
        .iter()
        .any(|resource| resource.name.as_str() == "argv_buf"));
    assert!(argv_buf_size_can_fulfill
        .iter()
        .any(|resource| resource.tref == args_get.params[1].tref));
}

#[test]
fn environ() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let environ_get = module.func(&Id::new("environ_get")).unwrap();
    let environ_sizes_get = module.func(&Id::new("environ_sizes_get")).unwrap();
    let environ_sizes_get_results = environ_sizes_get.unpack_expected_result();
    let environ_size_resource = environ_sizes_get_results[0].resource(&doc).unwrap();
    let environ_buf_size_resource = environ_sizes_get_results[1].resource(&doc).unwrap();
    let environ_size_can_fulfill = environ_size_resource.can_fulfill(&doc);
    let environ_buf_size_can_fulfill = environ_buf_size_resource.can_fulfill(&doc);

    assert!(environ_size_can_fulfill
        .iter()
        .any(|resource| resource.name.as_str() == "environ"));
    assert!(environ_size_can_fulfill
        .iter()
        .any(|resource| resource.tref == environ_get.params[0].tref));
    assert!(environ_buf_size_can_fulfill
        .iter()
        .any(|resource| resource.name.as_str() == "environ_buf"));
    assert!(environ_buf_size_can_fulfill
        .iter()
        .any(|resource| resource.tref == environ_get.params[1].tref));
}

#[test]
fn clock() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let clock_res_get = module.func(&Id::new("clock_res_get")).unwrap();
    let clock_time_get = module.func(&Id::new("clock_time_get")).unwrap();
    let res_tref = &clock_res_get.unpack_expected_result()[0];
    let res_resource = res_tref.resource(&doc).unwrap();
    let res_can_fulfill = res_resource.can_fulfill(&doc);
    let precision_tref = &clock_time_get.params[1].tref;

    assert!(
        res_can_fulfill
            .iter()
            .any(|resource| { &resource.tref == precision_tref }),
        "{:#?}",
        res_can_fulfill
    );
}

#[test]
fn fd_close() {
    let doc = document();
    let module = doc.module(&Id::new("wasi_snapshot_preview1")).unwrap();
    let fd_write = module.func(&Id::new("fd_close")).unwrap();
    let fd = &fd_write.params[0];

    assert!(fd.drop);
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
    let ciovec_record = match ciovec_tref.type_().as_ref() {
        | wazzi_witx::Type::Record(record) => record,
        | _ => panic!(),
    };
    let buf_resource = ciovec_record.members[0].tref.resource(&doc).unwrap();
    let buf_len_resource = ciovec_record.members[1].tref.resource(&doc).unwrap();
    let buf_len_can_fulfills = buf_len_resource.can_fulfill(&doc);

    assert!(buf_len_can_fulfills
        .iter()
        .any(|resource| resource.name == buf_resource.name));
}
