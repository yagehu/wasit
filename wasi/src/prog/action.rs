use std::{fs, io, path::PathBuf};

use color_eyre::eyre::{self, ContextCompat};
use strace::Strace;

#[derive(Debug, Clone)]
pub struct ActionStore {
    executor_pid: u32,
    root:         PathBuf,
    next_idx:     usize,
    recording:    Option<(usize, Strace)>,
}

impl ActionStore {
    pub fn with_existing_directory(
        path: PathBuf,
        executor_pid: u32,
    ) -> Result<ActionStore, io::Error> {
        Ok(Self {
            executor_pid,
            root: path.canonicalize()?,
            next_idx: 0,
            recording: None,
        })
    }
}

impl ActionStore {
    pub fn begin_action(&mut self) -> Result<(), eyre::Error> {
        let idx = self.next_idx;
        let action_dir = self.root.join(format!("{idx}"));
        let strace_output = action_dir.join("strace");

        fs::create_dir(action_dir)?;

        let strace = Strace::attach(self.executor_pid, &strace_output)?;

        self.recording = Some((idx, strace));
        self.next_idx += 1;

        Ok(())
    }

    fn end_action(&mut self) -> Result<(), eyre::Error> {
        let (_idx, strace) = self.recording.wrap_err("not recording action")?;

        Ok(())
    }

    fn num_actions(&self) -> Option<usize> {
        match self.recording {
            | Some(_) => None,
            | None => Some(self.next_idx),
        }
    }
}
