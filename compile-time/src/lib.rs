use std::path::PathBuf;

pub fn root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), ".."]
        .into_iter()
        .collect::<PathBuf>()
        .canonicalize()
        .unwrap()
}
