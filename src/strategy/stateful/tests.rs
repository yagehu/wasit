use std::collections::HashSet;

use tempfile::tempdir;

use super::*;
use crate::{resource::Resource, spec::RecordValue};

#[test]
fn ok() {
    let cfg = z3::Config::new();
    let ctx = z3::Context::new(&cfg);
    let solver = z3::Solver::new(&ctx);
    let spec = Spec::preview1().unwrap();
    let tempdir = tempdir().unwrap();
    let mut env = Environment::new();

    fs::write(tempdir.path().join("file"), &[]).unwrap();
    fs::create_dir(tempdir.path().join("dir")).unwrap();
    fs::write(tempdir.path().join("dir").join("nested"), &[]).unwrap();

    let dir_resource_idx = env.new_resource(
        "fd".to_string(),
        Resource {
            state: WasiValue::Record(RecordValue {
                members: vec![
                    WasiValue::U64(0),
                    spec.get_wasi_type("fdflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(HashSet::new()),
                    spec.get_wasi_type("filetype")
                        .unwrap()
                        .variant()
                        .unwrap()
                        .value_from_name("directory", None)
                        .unwrap(),
                    WasiValue::String("".as_bytes().to_vec()),
                    WasiValue::U64(0),
                ],
            }),
        },
    );
    let file_resource_idx = env.new_resource(
        "fd".to_string(),
        Resource {
            state: WasiValue::Record(RecordValue {
                members: vec![
                    WasiValue::U64(0),
                    spec.get_wasi_type("fdflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(HashSet::new()),
                    spec.get_wasi_type("filetype")
                        .unwrap()
                        .variant()
                        .unwrap()
                        .value_from_name("regular_file", None)
                        .unwrap(),
                    WasiValue::String("file".as_bytes().to_vec()),
                    WasiValue::U64(0),
                ],
            }),
        },
    );
    let mut state = State::new();
    let types = StateTypes::new(&ctx, &spec);

    state.push_preopen(dir_resource_idx, tempdir.path());
    state.push_resource(
        dir_resource_idx,
        spec.types.get_by_key("fd").unwrap(),
        env.resources.get(dir_resource_idx).unwrap().state.clone(),
    );
    state.push_resource(
        file_resource_idx,
        spec.types.get_by_key("fd").unwrap(),
        env.resources.get(file_resource_idx).unwrap().state.clone(),
    );

    let function = spec
        .interfaces
        .get_by_key("wasi_snapshot_preview1")
        .unwrap()
        .functions
        .get("path_open")
        .unwrap();
    let decls = state.declare(&spec, &ctx, &types, &env, function, None);
    let clause = state.encode(
        &ctx,
        &env,
        &types,
        &decls,
        &spec,
        function,
        None,
        function.input_contract.as_ref(),
    );

    solver.assert(&clause);

    assert_eq!(solver.check(), z3::SatResult::Sat);
}
