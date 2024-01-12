extern crate wazzi_witx as witx;

use std::{
    env,
    fs,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use arbitrary::Unstructured;
use tempfile::tempdir;

use wazzi_wasi::{prog, InMemorySnapshots, ProgSeed};

fn repo_root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), ".."].into_iter().collect()
}

fn spec() -> witx::Document {
    let dir = repo_root().join("spec").join("preview1").join("witx");

    witx::load(&[
        dir.join("typenames.witx"),
        dir.join("wasi_snapshot_preview1.witx"),
    ])
    .unwrap()
}

fn executor_bin() -> PathBuf {
    let profile = std::env!("OUT_DIR")
        .split(std::path::MAIN_SEPARATOR)
        .nth_back(3)
        .unwrap_or_else(|| "unknown")
        .to_string();

    repo_root()
        .join("target")
        .join(&profile)
        .join("wazzi-executor.wasm")
        .canonicalize()
        .unwrap()
}

#[test]
fn creat() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "00-creat.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut snapshots);

    let execute_result = seed.execute(&mut executor, &spec, &mut recorder);

    executor.kill();

    let stderr_str = String::from_utf8(stderr.try_lock().unwrap().deref().clone()).unwrap();

    assert!(
        execute_result.is_ok(),
        "{:#?}\n{}",
        execute_result,
        stderr_str
    );

    base_dir.path().join("a").canonicalize().expect(&format!(
        "00-creat seed should create file `a`\nexecutor stderr:\n{}\n",
        stderr_str,
    ));
}

#[test]
fn creat_write() {
    let spec = spec();
    let path = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "seeds",
        "01-creat_write.json",
    ]
    .into_iter()
    .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let stderr_str = String::from_utf8(stderr.try_lock().unwrap().deref().clone()).unwrap();
    let content = fs::read(base_dir.path().join("a").canonicalize().unwrap()).unwrap();

    assert_eq!(content, vec![97, 98], "{stderr_str}");
}

#[test]
fn args() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "02-args.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(wasmtime, executor_bin(), None)
        .run(stderr.clone())
        .expect("failed to run executor");
    let mut snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();
}

#[test]
fn environ() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "03-environ.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(wasmtime, executor_bin(), None)
        .run(stderr.clone())
        .expect("failed to run executor");
    let mut snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();
}

#[test]
fn clock() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "04-clock.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(wasmtime, executor_bin(), None)
        .run(stderr.clone())
        .expect("failed to run executor");
    let mut snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();
}

#[test]
fn read_after_write() {
    let spec = spec();
    let path = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "seeds",
        "05-read_after_write.json",
    ]
    .into_iter()
    .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut mem_snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut mem_snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let stderr_str = String::from_utf8(stderr.try_lock().unwrap().deref().clone()).unwrap();
    let fd_read_snapshot = &mem_snapshots.snapshots[3];

    assert!(matches!(fd_read_snapshot.errno, Some(0)));
    assert!(
        matches!(
            fd_read_snapshot.results[0].value,
            wazzi_wasi::Value::Builtin(wazzi_wasi::BuiltinValue::U32(2)),
        ),
        "{:#?}\nstderr:\n{}",
        fd_read_snapshot.results[0].value,
        stderr_str,
    );
}

#[test]
fn advise() {
    let spec = spec();
    let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "06-advise.json"]
        .into_iter()
        .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut mem_snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut mem_snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let fd_advise_snapshot = mem_snapshots.snapshots.last().unwrap();

    assert!(matches!(fd_advise_snapshot.errno, Some(0)));
}

#[test]
fn allocate() {
    let spec = spec();
    let path = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "seeds",
        "07-allocate.json",
    ]
    .into_iter()
    .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut mem_snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut mem_snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let fd_allocate_snapshot = mem_snapshots.snapshots.last().unwrap();
    let stderr_str = String::from_utf8(stderr.try_lock().unwrap().deref().clone()).unwrap();

    // Wasmtime no longer supports `fd_allocate`.
    // https://github.com/bytecodealliance/wasmtime/pull/6217
    assert!(
        matches!(fd_allocate_snapshot.errno, Some(58)),
        "snapshot:\n{:#?}\nstderr:\n{stderr_str}",
        fd_allocate_snapshot,
    );
}

#[test]
fn read_after_close() {
    let spec = spec();
    let path = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "seeds",
        "08-close_after_write.json",
    ]
    .into_iter()
    .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut mem_snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut mem_snapshots);
    let execute_result = seed.execute(&mut executor, &spec, &mut recorder);

    assert!(
        execute_result.is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let mut prog = execute_result.unwrap();

    // Since the fd was dropped via `fd_close`, it should be impossible to grow
    // the prog with say `fd_read` func because it only accepts a `newfd`.
    let grow_result = prog.grow_by_func(
        &mut Unstructured::new(&[]),
        &spec,
        &spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap()
            .func(&witx::Id::new("fd_read"))
            .unwrap(),
    );

    assert!(
        matches!(&grow_result, Err(prog::GrowError::NoResource { name }) if name == "newfd"),
        "{:#?}",
        grow_result
    );
}

#[test]
fn datasync() {
    let spec = spec();
    let path = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "seeds",
        "09-datasync.json",
    ]
    .into_iter()
    .collect::<PathBuf>();
    let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
    let seed: ProgSeed = serde_json::from_reader(f).unwrap();
    let base_dir = tempdir().unwrap();
    let wasmtime = wazzi_runners::Wasmtime::new("wasmtime");
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut executor = wazzi_executor::ExecutorRunner::new(
        wasmtime,
        executor_bin(),
        Some(base_dir.path().to_owned()),
    )
    .run(stderr.clone())
    .expect("failed to run executor");
    let mut mem_snapshots = InMemorySnapshots::default();
    let mut recorder = wazzi_wasi::Recorder::new(&mut mem_snapshots);

    assert!(
        seed.execute(&mut executor, &spec, &mut recorder).is_ok(),
        "Executor stderr:\n{}",
        String::from_utf8(stderr.lock().unwrap().deref().clone()).unwrap(),
    );

    executor.kill();

    let fd_datasync_snapshot = mem_snapshots.snapshots.last().unwrap();

    assert!(matches!(fd_datasync_snapshot.errno, Some(0)));
}
