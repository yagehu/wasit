use std::{fmt, fs, io, path::PathBuf};

use color_eyre::eyre::{self, eyre as err, Context, ContextCompat};
use serde::{Deserialize, Serialize};
use strace::Strace;

use super::Value;

pub struct ActionStore {
    executor_pid: u32,
    root:         PathBuf,
    next_idx:     usize,
    recording:    Option<(usize, Strace)>,
}

impl fmt::Debug for ActionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionStore")
            .field("executor_pid", &self.executor_pid)
            .field("root", &self.root)
            .field("next_idx", &self.next_idx)
            .field("recording", &self.recording.as_ref().map(|tuple| tuple.0))
            .finish()
    }
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
    pub fn begin_action(&mut self) -> Result<(), io::Error> {
        let idx = self.next_idx;
        let action_dir = self.root.join(format!("{idx}"));

        fs::create_dir(&action_dir)?;

        let strace_output = action_dir.join(OnDiskAction::STRACE_PATH);
        let strace = Strace::attach(self.executor_pid, &strace_output)?;

        self.recording = Some((idx, strace));
        self.next_idx += 1;

        Ok(())
    }

    pub fn end_action(&mut self, event: ActionCompletion) -> Result<(), eyre::Error> {
        let action_dir = self.recording_action_dir().unwrap();
        let (_idx, strace_inst) = self.recording.take().wrap_err("not recording action")?;

        strace_inst.stop()?;

        let trace_content = fs::read_to_string(action_dir.join(OnDiskAction::STRACE_PATH))
            .wrap_err("failed to read trace file")?;
        let trace = match strace::parse::trace(&trace_content) {
            | Ok((_rest, trace)) => trace,
            | Err(_err) => {
                let trace_content = trace_content.rsplitn(2, '\n').nth(1).unwrap_or_default();
                let (_rest, trace) = strace::parse::trace(trace_content)
                    .map_err(|_e| err!("failed to parse trace"))?;

                trace
            },
        };

        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(action_dir.join(OnDiskAction::STRACE_JSON_PATH))?,
            &trace,
        )?;
        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(action_dir.join(OnDiskAction::EVENT_PATH))?,
            &event,
        )?;

        Ok(())
    }

    fn recording_action_dir(&self) -> Option<PathBuf> {
        self.recording
            .as_ref()
            .map(|(idx, _)| self.root.join(format!("{idx}")))
    }

    fn num_actions(&self) -> usize {
        match self.recording {
            | Some(_) => self.next_idx - 1,
            | None => self.next_idx,
        }
    }
}

pub struct OnDiskAction {}

impl OnDiskAction {
    pub const EVENT_PATH: &'static str = "event.json";
    pub const STRACE_PATH: &'static str = "strace";
    pub const STRACE_JSON_PATH: &'static str = "strace.json";
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ActionCompletion {
    Decl,
    Call(CallCompletion),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallCompletion {
    pub errno:   Option<i32>,
    pub results: Vec<Value>,
}
