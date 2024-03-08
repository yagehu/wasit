use std::fs;

use wazzi_wasi::{prog::Value, seed};

use crate::common::{get_seed, run, run_seed};

#[test]
fn creat() {
    let run = run_seed("00-creat.json");
    let call = run
        .prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(call.errno, Some(0));
    assert!(run.base_dir.path().join("a").exists());
}

#[test]
fn write() {
    let run = run_seed("01-write.json");
    let call = run
        .prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(call.errno, Some(0));

    let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

    assert_eq!(file_content, vec![97, 98]);
}

#[test]
fn read_after_write() {
    let mut seed = get_seed("05-read_after_write.json");
    let size = 65537;

    match &mut seed.actions[1] {
        | seed::Action::Call(call) => {
            call.params[1] = seed::ResourceOrValue::Value(seed::Value::List(seed::ListValue(vec![
                seed::ResourceOrValue::Value(seed::Value::Record(seed::RecordValue(vec![
                    seed::RecordMemberValue {
                        name:  "buf".to_owned(),
                        value: seed::ResourceOrValue::Value(seed::Value::ConstPointer(
                            seed::ListValue(vec![
                                seed::ResourceOrValue::Value(
                                    seed::Value::Builtin(seed::BuiltinValue::U8(97))
                                );
                                size as usize
                            ]),
                        )),
                    },
                    seed::RecordMemberValue {
                        name:  "buf_len".to_owned(),
                        value: seed::ResourceOrValue::Value(seed::Value::Builtin(
                            seed::BuiltinValue::U32(size),
                        )),
                    },
                ]))),
            ])))
        },
        | _ => panic!(),
    }

    let run = run(seed);
    let prog = run.prog;
    let read_call = prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(read_call.errno, Some(0));
    assert!(
        matches!(
            read_call.results.last().unwrap(),
            &Value::Builtin(seed::BuiltinValue::U32(i)) if i == size,
        ),
        "stderr:{}\n{:#?}",
        run.stderr,
        read_call.results.first().unwrap()
    );
}

// #[test]
// fn advise() {
//     let run = run_seed("06-advise.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn allocate() {
//     let run = run_seed("07-allocate.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     // Wasmtime no longer supports `fd_allocate`.
//     // https://github.com/bytecodealliance/wasmtime/pull/6217
//     assert_eq!(prog.calls.last().unwrap().errno, Some(58));
// }

// #[test]
// fn close() {
//     let run = run_seed("08-close.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn datasync() {
//     let run = run_seed("09-datasync.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn fdstat_get() {
//     let run = run_seed("10-fdstat_get.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn fdstat_set_flags() {
//     let run = run_seed("11-fdstat_set_flags.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn fdstat_set_rights() {
//     let run = run_seed("12-fdstat_set_rights.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn filestat_get() {
//     let run = run_seed("13-filestat_get.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn filestat_set_size() {
//     let run = run_seed("14-filestat_set_size.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn filestat_set_times() {
//     let run = run_seed("15-filestat_set_times.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn pread() {
//     let mut seed = get_seed("16-pread.json");
//     let size = 32;
//     let mut file_content = vec![SeedValue::Builtin(seed::BuiltinValue::U8(97)); size as usize - 1];

//     file_content.push(SeedValue::Builtin(seed::BuiltinValue::U8(98)));

//     match &mut seed.actions[1] {
//         | seed::Action::Call(call) => {
//             call.params[1] =
//                 seed::ParamSpec::Value(SeedValue::List(seed::ListValue(vec![SeedValue::Record(
//                     seed::RecordValue(vec![
//                         seed::RecordMember {
//                             name:  "buf".to_owned(),
//                             value: ParamSpec::Value(SeedValue::ConstPointer(seed::ListValue(
//                                 file_content,
//                             ))),
//                         },
//                         seed::RecordMember {
//                             name:  "buf_len".to_owned(),
//                             value: ParamSpec::Value(SeedValue::Builtin(seed::BuiltinValue::U32(
//                                 size,
//                             ))),
//                         },
//                     ]),
//                 )])))
//         },
//         | _ => panic!(),
//     }

//     let run = run(seed);
//     let prog = run.result.expect(&run.stderr).finish(&spec());
//     let read_call = prog.calls.last().unwrap();

//     assert_eq!(read_call.errno, Some(0));
//     assert!(
//         matches!(
//             read_call.results.last().unwrap(),
//             &Value::Builtin(seed::BuiltinValue::U32(i)) if i == size,
//         ),
//         "stderr:{}\n{:#?}",
//         run.stderr,
//         read_call.results.first().unwrap()
//     );

//     let iovec = match &read_call.params_post[1] {
//         | Value::List(values) => &values[0],
//         | _ => panic!(),
//     };
//     let buf = match &iovec {
//         | Value::Record(record) => &record.0[0].value,
//         | _ => panic!(),
//     };
//     let values = match &buf {
//         | Value::Pointer(values) => values,
//         | _ => panic!(),
//     };

//     assert_eq!(values.len(), size as usize);
//     assert!(
//         matches!(
//             values.last().unwrap(),
//             Value::Builtin(seed::BuiltinValue::U8(98)),
//         ),
//         "{}\n{:#?}",
//         run.stderr,
//         values.last()
//     );
// }

// #[test]
// fn prestat_get() {
//     let run = run_seed("17-prestat_get.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn prestat_dir_name() {
//     let run = run_seed("18-prestat_dir_name.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());
//     let call = prog.calls.last().unwrap();

//     assert_eq!(call.errno, Some(0), "{}", run.stderr);

//     let values = match &call.params_post[1] {
//         | Value::Pointer(values) => values,
//         | _ => panic!(),
//     };
//     let bytes = values
//         .iter()
//         .map(|value| match value {
//             | Value::Builtin(builtin) => match builtin {
//                 | &seed::BuiltinValue::U8(i) => i,
//                 | _ => panic!(),
//             },
//             | _ => panic!(),
//         })
//         .collect::<Vec<_>>();
//     let string = String::from_utf8(bytes).unwrap();

//     assert!(string.ends_with(&run.base_dir.path().to_string_lossy().to_string()));
// }

// #[test]
// fn pwrite() {
//     let run = run_seed("19-pwrite.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));

//     let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

//     assert_eq!(&file_content, &[97, 98]);
// }

// #[test]
// fn fd_readdir() {
//     let run = run_seed("20-fd_readdir.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn fd_renumber() {
//     let run = run_seed("21-fd_renumber.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));

//     let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

//     assert_eq!(&file_content, &[97, 98]);
// }

// #[test]
// fn fd_sync() {
//     let run = run_seed("22-fd_sync.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn fd_tell() {
//     let run = run_seed("23-fd_tell.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn path_create_directory() {
//     let run = run_seed("24-path_create_directory.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
//     assert!(run.base_dir.path().join("a").is_dir());
// }

// #[test]
// fn path_filestat_get() {
//     let run = run_seed("25-path_filestat_get.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn path_filestat_set_times() {
//     let run = run_seed("26-path_filestat_set_times.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn path_link() {
//     let run = run_seed("27-path_link.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
// }

// #[test]
// fn path_remove_directory() {
//     let run = run_seed("28-path_remove_directory.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
//     assert!(!run.base_dir.path().join("a").exists());
// }

// #[test]
// fn path_rename() {
//     let run = run_seed("29-path_rename.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
//     assert!(!run.base_dir.path().join("a").exists());
//     assert!(run.base_dir.path().join("b").exists());
// }

// #[test]
// fn path_symlink() {
//     let run = run_seed("30-path_symlink.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
//     assert_eq!(
//         run.base_dir.path().join("a").canonicalize().unwrap(),
//         run.base_dir.path().join("b").canonicalize().unwrap(),
//     );
// }

// #[test]
// fn path_unlink_file() {
//     let run = run_seed("31-path_unlink_file.json");
//     let prog = run.result.expect(&run.stderr).finish(&spec());

//     assert_eq!(prog.calls.last().unwrap().errno, Some(0));
//     assert!(!run.base_dir.path().join("a").exists());
// }
