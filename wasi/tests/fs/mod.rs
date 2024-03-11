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
    let seed = get_seed("05-read_after_write.json");
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
            &Value::Builtin(seed::BuiltinValue::U32(1)),
        ),
        "stderr:{}",
        run.stderr,
    );
}

#[test]
fn advise() {
    let run = run_seed("06-advise.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn allocate() {
    let run = run_seed("07-allocate.json");

    // Wasmtime no longer supports `fd_allocate`.
    // https://github.com/bytecodealliance/wasmtime/pull/6217
    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(58)
    );
}

#[test]
fn close() {
    let run = run_seed("08-close.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn datasync() {
    let run = run_seed("09-datasync.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn fdstat_get() {
    let run = run_seed("10-fdstat_get.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn fdstat_set_flags() {
    let run = run_seed("11-fdstat_set_flags.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn fdstat_set_rights() {
    let run = run_seed("12-fdstat_set_rights.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn filestat_get() {
    let run = run_seed("13-filestat_get.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn filestat_set_size() {
    let run = run_seed("14-filestat_set_size.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn filestat_set_times() {
    let run = run_seed("15-filestat_set_times.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn pread() {
    let run = run_seed("16-pread.json");
    let read_call = run
        .prog
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
            &Value::Builtin(seed::BuiltinValue::U32(2)),
        ),
        "stderr:{}\n{:#?}",
        run.stderr,
        read_call.results.first().unwrap()
    );

    let iovec = match &read_call.params[1] {
        | Value::List(values) => &values[0],
        | _ => panic!(),
    };
    let buf = match &iovec {
        | Value::Record(record) => &record.members[0].value,
        | _ => panic!(),
    };
    let values = match &buf {
        | Value::Pointer(values) => values,
        | _ => panic!(),
    };

    assert_eq!(values.len(), 2);
    assert!(
        matches!(
            values.last().unwrap(),
            Value::Builtin(seed::BuiltinValue::U8(98)),
        ),
        "{}\n{:#?}",
        run.stderr,
        values.last()
    );
}

#[test]
fn prestat_get() {
    let run = run_seed("17-prestat_get.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn prestat_dir_name() {
    let run = run_seed("18-prestat_dir_name.json");
    let call = run
        .prog
        .store()
        .recorder()
        .last()
        .unwrap()
        .unwrap()
        .read_result()
        .unwrap();

    assert_eq!(call.errno, Some(0), "{}", run.stderr);

    let values = match &call.params[1] {
        | Value::Pointer(values) => values,
        | _ => panic!(),
    };
    let bytes = values
        .iter()
        .map(|value| match value {
            | Value::Builtin(builtin) => match builtin {
                | &seed::BuiltinValue::U8(i) => i,
                | _ => panic!(),
            },
            | _ => panic!(),
        })
        .collect::<Vec<_>>();
    let string = String::from_utf8(bytes).unwrap();

    assert!(string.ends_with(&run.base_dir.path().to_string_lossy().to_string()));
}

#[test]
fn pwrite() {
    let run = run_seed("19-pwrite.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );

    let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

    assert_eq!(&file_content, &[97, 98]);
}

#[test]
fn fd_readdir() {
    let run = run_seed("20-fd_readdir.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn fd_renumber() {
    let run = run_seed("21-fd_renumber.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );

    let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

    assert_eq!(&file_content, &[97, 98]);
}

#[test]
fn fd_sync() {
    let run = run_seed("22-fd_sync.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

#[test]
fn fd_tell() {
    let run = run_seed("23-fd_tell.json");

    assert_eq!(
        run.prog
            .store()
            .recorder()
            .last()
            .unwrap()
            .unwrap()
            .read_result()
            .unwrap()
            .errno,
        Some(0)
    );
}

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
