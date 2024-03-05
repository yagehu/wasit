use std::{fs, path::PathBuf};

use color_eyre::eyre::{self, Context};

use crate::prog::call::CallRecorder;

#[derive(Debug)]
pub struct ExecutionStore {
    root:     PathBuf,
    recorder: CallRecorder,
}

impl ExecutionStore {
    pub fn new(
        root: PathBuf,
        runtime_version: &str,
        executor_pid: u32,
    ) -> Result<Self, eyre::Error> {
        let root = root.canonicalize()?;
        let calls_dir = root.join("calls");
        // let repo = git2::Repository::open(repo_path).wrap_err("failed to open runtime repo")?;
        // let head_ref = repo.head().wrap_err("failed to get head reference")?;
        // let head_commit = head_ref
        //     .peel_to_commit()
        //     .wrap_err("failed to get commit for head ref")?;

        fs::write(root.join("version"), runtime_version)?;

        let recorder = CallRecorder::new(&calls_dir, executor_pid)
            .wrap_err("failed to instantiate call recorder")?;

        Ok(Self { root, recorder })
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
