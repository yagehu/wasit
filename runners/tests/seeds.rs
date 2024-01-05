// use std::{fs, io, path::PathBuf};

// use wazzi_runners::WasiRunner;
// use wazzi_wasi::ProgSeed;

// #[test]
// #[cfg_attr(not(feature = "test-runtimes"), ignore)]
// fn creat() {
//     let path = [env!("CARGO_MANIFEST_DIR"), "..", "seeds", "00-creat.json"]
//         .into_iter()
//         .collect::<PathBuf>();
//     let f = fs::OpenOptions::new().read(true).open(&path).unwrap();
//     let seed: ProgSeed = serde_json::from_reader(f).unwrap();
//     let wasmtime_path = PathBuf::from("wasmtime").canonicalize().unwrap();
//     let wasmtime = wazzi_runners::Wasmtime::new(wasmtime_path);

//     wasmtime.run(wasm_path, base_dir, stderr_logger)
// }
