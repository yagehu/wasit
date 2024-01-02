use wazzi_witx::{Id, Resource, TypeRef};

fn document() -> wazzi_witx::Document {
    wazzi_witx::load(&[
        "../spec/preview1/witx/typenames.witx",
        "../spec/preview1/witx/wasi_snapshot_preview1.witx",
    ])
    .unwrap()
}

fn unwrap_tref_resource(tref: &TypeRef) -> &Id {
    match tref {
        TypeRef::Name(named_type) => match &named_type.resource {
            Some(resource) => resource,
            None => panic!("tref is not a resource: {:#?}", tref),
        },
        _ => panic!(),
    }
}

#[test]
fn resources_exist() {
    let doc = document();
    let cases = vec!["argv", "argv_buf", "argv_size", "argv_buf_size"];

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
    let argv_size_resource_name = unwrap_tref_resource(&args_sizes_get_results[0]);
    let argv_buf_size_resource_name = unwrap_tref_resource(&args_sizes_get_results[1]);
    let argv_size_resource = doc.resource(argv_size_resource_name).unwrap();
    let argv_buf_size_resource = doc.resource(argv_buf_size_resource_name).unwrap();
    let argv_size_can_fulfill = argv_size_resource.can_fulfill(&doc);

    assert_eq!(
        argv_size_can_fulfill,
        vec![&Resource {
            name: Id::new("argv"),
            tref: todo!()
        }]
    );
}
