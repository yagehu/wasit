use std::path::PathBuf;

pub fn root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), ".."].into_iter().collect::<PathBuf>()
}

pub const GIT_HASH: &str = env!("GIT_HASH");
