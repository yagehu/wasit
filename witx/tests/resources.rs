use wazzi_witx::Id;

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
    let cases = vec![
        "argv",
        "argv_buf",
        "argv_size",
        "argv_buf_size",
        "fd",
        "environ",
        "environ_size",
        "environ_buf",
        "environ_buf_size",
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

    assert_eq!(argv_size_can_fulfill[0].name, Id::new("argv"));
    assert_eq!(argv_size_can_fulfill[0].tref, args_get.params[0].tref);
    assert_eq!(argv_buf_size_can_fulfill[0].name, Id::new("argv_buf"));
    assert_eq!(argv_buf_size_can_fulfill[0].tref, args_get.params[1].tref);
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

    assert_eq!(environ_size_can_fulfill[0].name, Id::new("environ"));
    assert_eq!(environ_size_can_fulfill[0].tref, environ_get.params[0].tref);
    assert_eq!(environ_buf_size_can_fulfill[0].name, Id::new("environ_buf"));
    assert_eq!(
        environ_buf_size_can_fulfill[0].tref,
        environ_get.params[1].tref
    );
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

    assert!(buf_len_can_fulfills
        .iter()
        .any(|resource| resource.name == buf_resource.name));
}
