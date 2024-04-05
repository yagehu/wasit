use eyre::{eyre as err, Context, ContextCompat};
use std::{
    fs,
    io::BufReader,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tempfile::{tempdir, TempDir};
use wazi::{prog::Prog, seed::Seed};
use wazzi_executor::ExecutorRunner;
use wazzi_runners::Wasmtime;
use wazzi_spec::{package::Package, parsers::Span};
use wazzi_store::RuntimeStore;
use wazzi_wasi_component_model::value::Value;

fn main() -> Result<(), eyre::Error> {
    struct Case {
        seed:   &'static str,
        spec:   &'static str,
        assert: Box<dyn FnOnce(RunInstance) -> Result<(), eyre::Error>>,
    }

    color_eyre::install()?;

    let cases = [
        Case {
            seed:   "00-creat.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0), "{}", run.stderr);
                assert!(run.base_dir.path().join("a").exists());

                Ok(())
            }),
        },
        Case {
            seed:   "01-write.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));
                assert!(run.base_dir.path().join("a").exists());

                Ok(())
            }),
        },
        Case {
            seed:   "05-read_after_write.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));
                assert!(
                    matches!(call.results.last().unwrap(), &Value::U32(1)),
                    "{:?}\n{}",
                    call.results,
                    run.stderr
                );

                Ok(())
            }),
        },
        Case {
            seed:   "06-advise.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));

                Ok(())
            }),
        },
        Case {
            seed:   "07-fd_allocate.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(58));

                Ok(())
            }),
        },
        Case {
            seed:   "08-fd_close.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));

                Ok(())
            }),
        },
        Case {
            seed:   "09-fd_datasync.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));

                Ok(())
            }),
        },
        Case {
            seed:   "10-fd_fdstat_get.json",
            spec:   "preview1.witx",
            assert: Box::new(|run| {
                let action = run
                    .prog
                    .store()
                    .trace()
                    .last_call()
                    .unwrap()
                    .wrap_err("no last call")?
                    .read()
                    .unwrap();
                let call = action.call().unwrap();

                assert_eq!(call.errno, Some(0));

                Ok(())
            }),
        },
    ];

    for (i, case) in cases.into_iter().enumerate() {
        let err_handler = || format!("case {i} {} failed", case.seed);
        let run = run_seed(case.seed, case.spec).wrap_err_with(err_handler)?;

        (case.assert)(run).wrap_err_with(err_handler)?;
    }

    Ok(())
}

pub fn get_seed(name: &str) -> Result<Seed, eyre::Error> {
    Ok(serde_json::from_reader(BufReader::new(
        fs::OpenOptions::new()
            .read(true)
            .open(wazzi_compile_time::root().join("seeds").join(name))
            .unwrap(),
    ))?)
}

fn executor_bin() -> PathBuf {
    let profile = env!("OUT_DIR")
        .split(std::path::MAIN_SEPARATOR)
        .nth_back(3)
        .unwrap_or_else(|| "unknown")
        .to_string();

    wazzi_compile_time::root()
        .join("target")
        .join(&profile)
        .join("wazzi-executor-pb.wasm")
        .canonicalize()
        .unwrap()
}

#[derive(Debug)]
pub struct RunInstance {
    pub base_dir: TempDir,
    pub prog:     Prog,
    pub stderr:   String,
}

pub fn run_seed(name: &str, spec: &str) -> Result<RunInstance, eyre::Error> {
    let seed = get_seed(name).wrap_err("failed to get seed")?;

    run(seed, spec)
}

pub fn run(seed: Seed, spec_name: &str) -> Result<RunInstance, eyre::Error> {
    let base_dir = tempdir().unwrap();
    let wasmtime = Wasmtime::new(Path::new("wasmtime"));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let executor = ExecutorRunner::new(
        &wasmtime,
        executor_bin(),
        base_dir.path().to_path_buf(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let path = base_dir.path().to_owned();
    let store = RuntimeStore::new(&path, "test").unwrap();
    let run = thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();

        scope.spawn({
            let executor = executor.clone();
            let spec = spec(spec_name)?;

            move || {
                let result = seed.execute(&spec, executor, store);

                tx.send(result).unwrap();
            }
        });

        let result = rx.recv_timeout(Duration::from_millis(60000));
        let prog = match result {
            | Ok(result) => result?,
            | Err(err) => {
                executor.kill();
                let s = String::from_utf8(stderr.lock().unwrap().to_vec()).unwrap();
                eprintln!("{s}");
                panic!("Execution timeout or error.\nerr:\n{}", err)
            },
        };

        prog.executor().kill();

        Ok(RunInstance {
            base_dir,
            prog,
            stderr: String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
        })
    });

    run
}

pub fn spec(name: &str) -> Result<Package, eyre::Error> {
    let spec_dir = wazzi_compile_time::root().join("spec");
    let spec = fs::read_to_string(spec_dir.join(name)).unwrap();
    let result = wazzi_spec::parsers::wazzi_preview1::Document::parse(Span::new(&spec));
    let document = match result {
        | Ok(doc) => doc,
        | Err(err) => {
            eprintln!("{err}");
            return Err(err!("failed to parse document"));
        },
    };

    Ok(document.into_package().unwrap())
}
