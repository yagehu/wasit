mod common;

use std::fs;

use wazzi_wasi::{
    prog::seed::{self, ParamSpec, SeedValue},
    Value,
};

use crate::common::{get_seed, run, run_seed};

#[test]
fn creat() {
    let run = run_seed("00-creat.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));
    assert!(run.base_dir.path().join("a").exists());
}

#[test]
fn write() {
    let run = run_seed("01-write.json");
    let prog = run.result.expect(&run.stderr).finish();

    assert_eq!(prog.calls.last().unwrap().errno, Some(0));

    let file_content = fs::read(run.base_dir.path().join("a")).unwrap();

    assert_eq!(file_content, vec![97, 98]);
}

#[test]
fn read_after_write() {
    let mut seed = get_seed("05-read_after_write.json");
    let size = 65537;

    match &mut seed.actions[1] {
        | seed::Action::Call(call) => {
            call.params[1] =
                seed::ParamSpec::Value(SeedValue::List(seed::ListValue(vec![SeedValue::Record(
                    seed::RecordValue(vec![
                        seed::RecordMember {
                            name:  "buf".to_owned(),
                            value: ParamSpec::Value(SeedValue::ConstPointer(seed::ListValue(
                                vec![SeedValue::Builtin(seed::BuiltinValue::U8(97)); size as usize],
                            ))),
                        },
                        seed::RecordMember {
                            name:  "buf_len".to_owned(),
                            value: ParamSpec::Value(SeedValue::Builtin(seed::BuiltinValue::U32(
                                size,
                            ))),
                        },
                    ]),
                )])))
        },
        | _ => panic!(),
    }

    let run = run(seed);
    let prog = run.result.expect(&run.stderr).finish();
    let read_call = prog.calls.last().unwrap();

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
