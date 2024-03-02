use std::{fmt, fs, io, path::PathBuf};

use color_eyre::eyre::{self, eyre as err, Context, ContextCompat};
use nom::combinator::all_consuming;
use serde::{Deserialize, Serialize};
use strace::{parse::Trace, Strace};

use super::Value;

pub struct CallStore {
    executor_pid: u32,
    root:         PathBuf,
    next_idx:     usize,
    recording:    Option<(usize, Strace)>,
}

impl fmt::Debug for CallStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallStore")
            .field("executor_pid", &self.executor_pid)
            .field("root", &self.root)
            .field("next_idx", &self.next_idx)
            .field("recording", &self.recording.as_ref().map(|tuple| tuple.0))
            .finish()
    }
}

impl CallStore {
    pub fn with_existing_directory(
        path: PathBuf,
        executor_pid: u32,
    ) -> Result<CallStore, io::Error> {
        Ok(Self {
            executor_pid,
            root: path.canonicalize()?,
            next_idx: 0,
            recording: None,
        })
    }
}

impl CallStore {
    pub fn begin_call(&mut self) -> Result<(), io::Error> {
        let idx = self.next_idx;
        let dir = self.root.join(format!("{idx}"));

        fs::create_dir(&dir)?;

        let strace_output = dir.join(OnDiskCall::STRACE_PATH);
        let strace = Strace::attach(self.executor_pid, &strace_output)?;

        self.recording = Some((idx, strace));

        Ok(())
    }

    pub fn end_call(&mut self, result: CallResult) -> Result<(), eyre::Error> {
        let action_dir = self.recording_action_dir().unwrap();
        let (_idx, strace_inst) = self.recording.take().wrap_err("not recording action")?;

        strace_inst.stop()?;

        let trace_content = fs::read_to_string(action_dir.join(OnDiskCall::STRACE_PATH))
            .wrap_err("failed to read trace file")?;
        let (_rest, trace) = all_consuming(Trace::parse)(&trace_content)
            .map_err(|_| err!("failed to parse trace"))?;

        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(action_dir.join(OnDiskCall::RESULT_JSON_PATH))?,
            &result,
        )?;
        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(action_dir.join(OnDiskCall::TRACE_JSON_PATH))?,
            &trace,
        )?;

        self.next_idx += 1;

        Ok(())
    }

    pub fn last(&self) -> Result<Option<OnDiskCall>, io::Error> {
        if self.next_idx == 0 {
            return Ok(None);
        }

        let path = self
            .root
            .join(format!("{}", self.next_idx - 1))
            .canonicalize()?;

        Ok(Some(OnDiskCall::with_existing_directory(path)?))
    }

    fn recording_action_dir(&self) -> Option<PathBuf> {
        self.recording
            .as_ref()
            .map(|(idx, _)| self.root.join(format!("{idx}")))
    }
}

#[derive(Debug)]
pub struct OnDiskCall {
    path: PathBuf,
}

impl OnDiskCall {
    pub(crate) const RESULT_JSON_PATH: &'static str = "result.json";
    pub(crate) const TRACE_JSON_PATH: &'static str = "trace.json";
    pub(crate) const STRACE_PATH: &'static str = "strace";

    pub fn with_existing_directory(path: PathBuf) -> Result<Self, io::Error> {
        Ok(Self {
            path: path.canonicalize()?,
        })
    }

    pub fn read_result(&self) -> Result<CallResult, io::Error> {
        let f = fs::OpenOptions::new()
            .read(true)
            .open(&self.path.join(Self::RESULT_JSON_PATH))?;
        let call = serde_json::from_reader(f)?;

        Ok(call)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallResult {
    pub func:    String,
    pub errno:   Option<i32>,
    pub params:  Vec<Value>,
    pub results: Vec<Value>,
}
