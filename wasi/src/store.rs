use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context};

use crate::prog::call::CallRecorder;

#[derive(Debug)]
pub struct ExecutionStore {
    root:     Box<Path>,
    recorder: CallRecorder,
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

    pub fn path(&self) -> &Path {
        &self.root
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
