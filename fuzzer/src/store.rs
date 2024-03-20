use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct FuzzStore {
    path:      PathBuf,
    data_file: PathBuf,
}

impl FuzzStore {
    pub fn new(path: &Path) -> Result<Self, io::Error> {
        let path = path.canonicalize()?;
        let data_file = path.join("data");

        Ok(Self { path, data_file })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write_seed_data(&self, data: &[u8]) -> Result<(), io::Error> {
        fs::write(&self.data_file, data)
    }
}
