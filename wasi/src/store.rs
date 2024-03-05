use std::{
    fs,
    mem,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context};

use crate::prog::call::CallRecorder;

#[derive(Debug)]
pub struct ExecutionStore {
    root:     Box<Path>,
    recorder: CallRecorder,
}

impl Drop for ExecutionStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl ExecutionStore {
    pub fn new(root: &Path, runtime_version: &str, executor_pid: u32) -> Result<Self, eyre::Error> {
        let root = root.canonicalize()?.into_boxed_path();
        let calls_dir = root.join("calls");

        fs::write(root.join("version"), runtime_version)?;

        let recorder = CallRecorder::new(&calls_dir, executor_pid)
            .wrap_err("failed to instantiate call recorder")?;

        Ok(Self { root, recorder })
    }

    pub fn into_path(self) -> PathBuf {
        // Prevent the Drop impl from being called.
        let mut this = mem::ManuallyDrop::new(self);

        // Replace this.path with an empty Box, since an empty Box does not
        // allocate any heap memory.
        mem::replace(&mut this.root, PathBuf::new().into_boxed_path()).into()
    }

    pub fn prog_path(&self) -> PathBuf {
        self.root.join("prog")
    }

    pub fn recorder(&self) -> &CallRecorder {
        &self.recorder
    }

    pub fn recorder_mut(&mut self) -> &mut CallRecorder {
        &mut self.recorder
    }
}
