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
    state.push_path(
        "path".to_string(),
        PathString {
            param_name: "path".to_owned(),
            nsegments:  3,
        },
    );

    let function = spec
        .interfaces
        .get_by_key("wasi_snapshot_preview1")
        .unwrap()
        .functions
        .get("path_open")
        .unwrap();
    let decls = state.declare(&spec, &ctx, &types, &env, function);
    let clause = state.encode(&ctx, &env, &types, &decls, &spec, function);

    solver.assert(&clause);

    assert_eq!(solver.check(), z3::SatResult::Sat);

    {
        solver.push();

        let fd_datatype = types.resources.get("fd").unwrap();
        let path_datatype = types.resources.get("path").unwrap();
        let some_fd = z3::ast::Dynamic::fresh_const(&ctx, "sol-fd", &fd_datatype.sort);

        solver.assert(&z3::ast::Bool::and(
            &ctx,
            &[
                &fd_datatype.variants[0]
                    .tester
                    .apply(&[&some_fd])
                    .as_bool()
                    .unwrap(),
                &path_datatype.variants[0].accessors[0]
                    .apply(&[&fd_datatype.variants[0].accessors[3]
                        .apply(&[&some_fd])
                        .as_datatype()
                        .unwrap()])
                    .as_string()
                    .unwrap()
                    ._eq(&z3::ast::String::from_str(&ctx, "file").unwrap()),
                &z3::ast::Bool::or(
                    &ctx,
                    env.resources_by_types
                        .get("fd")
                        .unwrap()
                        .iter()
                        .map(|&idx| decls.resources.get(&idx).unwrap()._eq(&some_fd))
                        .collect_vec()
                        .as_slice(),
                ),
            ],
        ));

        assert_eq!(solver.check(), z3::SatResult::Sat);

        let model = solver.get_model().unwrap();
        let fd_tdef = spec.types.get_by_key("fd").unwrap();
        let resource_value = state.decode_to_wasi_value(
            &ctx,
            &spec,
            &types,
            fd_tdef,
            &model.eval(&some_fd, true).unwrap().simplify(),
        );
        let resource_idx = *env
            .reverse_resource_index
            .get("fd")
            .unwrap()
            .get(&resource_value)
            .unwrap();

        assert_eq!(resource_idx.0, 1);

        solver.pop(1);
    }

    {
        solver.push();

        let path_encoding = decls.paths.get("path").unwrap();

        solver.assert(&z3::ast::Bool::and(
            &ctx,
            &[
                &decls
                    .fd_file
                    .apply(&[
                        decls.params.get("fd").unwrap(),
                        &decls.preopens.get(&dir_resource_idx).unwrap().root.node,
                    ])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1]
                    .tester
                    .apply(&[&path_encoding.segments[0]])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1].accessors[0]
                    .apply(&[&path_encoding.segments[0]])
                    .as_string()
                    .unwrap()
                    ._eq(&z3::ast::String::from_str(&ctx, "dir").unwrap())
                    .not(),
                &types.segment.variants[1].accessors[0]
                    .apply(&[&path_encoding.segments[0]])
                    .as_string()
                    .unwrap()
                    ._eq(&z3::ast::String::from_str(&ctx, "file").unwrap())
                    .not(),
                &types.segment.variants[0]
                    .tester
                    .apply(&[&path_encoding.segments[1]])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1]
                    .tester
                    .apply(&[&path_encoding.segments[2]])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1].accessors[0]
                    .apply(&[&path_encoding.segments[2]])
                    .as_string()
                    .unwrap()
                    ._eq(&z3::ast::String::from_str(&ctx, "..").unwrap()),
            ],
        ));

        assert_eq!(solver.check(), z3::SatResult::Unsat);

        solver.pop(1);
    }

    {
        solver.push();

        let path_encoding = decls.paths.get("path").unwrap();

        solver.assert(&z3::ast::Bool::and(
            &ctx,
            &[
                &decls
                    .fd_file
                    .apply(&[
                        decls.params.get("fd").unwrap(),
                        &decls.preopens.get(&dir_resource_idx).unwrap().root.node,
                    ])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1]
                    .tester
                    .apply(&[&path_encoding.segments[0]])
                    .as_bool()
                    .unwrap(),
                &types.segment.variants[1].accessors[0]
                    .apply(&[&path_encoding.segments[0]])
                    .as_string()
                    .unwrap()
                    ._eq(&z3::ast::String::from_str(&ctx, "dir").unwrap()),
                &types.segment.variants[1]
                    .tester
                    .apply(&[&path_encoding.segments[2]])
                    .as_bool()
                    .unwrap(),
                // &types.segment.variants[1].accessors[0]
                //     .apply(&[&path_encoding.segments[2]])
                //     .as_string()
                //     .unwrap()
                //     ._eq(&z3::ast::String::from_str(&ctx, "..").unwrap()),
            ],
        ));

        assert_eq!(solver.check(), z3::SatResult::Sat);

        solver.pop(1);
    }
}
