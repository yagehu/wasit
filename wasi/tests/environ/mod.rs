use crate::common::run_seed;

#[test]
fn ok() {
    let run = run_seed("03-environ.json");
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
}
