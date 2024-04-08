use std::{
    collections::HashSet,
    fs,
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};

use eyre::{Context, ContextCompat as _};
use serde::{Deserialize, Serialize};
use wazzi_wasi_component_model::value::{Value, ValueMeta};

#[derive(Clone, Debug)]
pub struct FuzzStore {
    path: PathBuf,
    next: usize,
}

impl FuzzStore {
    pub fn new(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;

        Ok(Self { path, next: 0 })
    }

    pub fn new_run(&mut self) -> Result<RunStore, io::Error> {
        let run_idx = self.next;

        self.next += 1;

        RunStore::new(&self.path.join(format!("{:03}", run_idx)))
    }
}

#[derive(Clone, Debug)]
pub struct RunStore {
    path:         PathBuf,
    runtimes_dir: PathBuf,
    runtimes:     HashSet<String>,
}

impl RunStore {
    pub fn new(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir(path)?;

        let path = path.canonicalize()?;
        let runtimes_dir = path.join("runtimes");

        fs::create_dir(&runtimes_dir)?;

        Ok(Self {
            path,
            runtimes_dir,
            runtimes: Default::default(),
        })
    }

    pub fn new_runtime(&mut self, name: &str, version: &str) -> Result<RuntimeStore, io::Error> {
        let store = RuntimeStore::new(&self.runtimes_dir.join(name), version)?;

        self.runtimes.insert(name.to_owned());

        Ok(store)
    }

    pub fn write_data(&self, data: &[u8]) -> Result<(), io::Error> {
        fs::write(self.path.join("data"), data)
    }

    pub fn runtimes(&self) -> Result<impl Iterator<Item = RuntimeStore> + '_, io::Error> {
        Ok(self
            .runtimes
            .iter()
            .map(|runtime| RuntimeStore::resume(&self.runtimes_dir.join(runtime)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter())
    }
}

#[derive(Debug)]
pub struct RuntimeStore {
    pub path: PathBuf,
    pub base: PathBuf,

    version_path: PathBuf,
    trace:        TraceStore,
}

impl RuntimeStore {
    pub fn resume(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;
        let base = path.join("base");
        let version_path = path.join("version");
        let trace = TraceStore::resume(&path.join("trace"))?;

        Ok(Self {
            path,
            base,
            version_path,
            trace,
        })
    }

    pub fn new(path: &Path, version: &str) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;
        let base = path.join("base");
        let version_path = path.join("version");
        let trace = TraceStore::new(&path.join("trace"))?;

        fs::create_dir_all(&base)?;
        fs::write(&version_path, version)?;

        Ok(Self {
            path,
            base,
            version_path,
            trace,
        })
    }

    pub fn trace(&self) -> &TraceStore {
        &self.trace
    }

    pub fn trace_mut(&mut self) -> &mut TraceStore {
        &mut self.trace
    }
}

#[derive(Debug)]
pub struct TraceStore {
    path:      PathBuf,
    next:      usize,
    recording: Option<usize>,
}

impl TraceStore {
    pub fn resume(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;
        let mut next = 0;

        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let idx = match entry
                .file_name()
                .to_string_lossy()
                .to_string()
                .parse::<usize>()
            {
                | Ok(idx) => idx,
                | Err(_) => continue,
            };

            next = next.max(idx + 1);
        }

        Ok(Self {
            path,
            next,
            recording: None,
        })
    }

    pub fn new(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir(path)?;

        let path = path.canonicalize()?;

        Ok(Self {
            path,
            next: 0,
            recording: None,
        })
    }

    pub fn last_call(&self) -> Result<Option<ActionStore>, io::Error> {
        if self.next == 0 {
            return Ok(None);
        }

        for i in (0..self.next).rev() {
            let dir = self.action_path(i);
            let action_store = ActionStore::from_path(&dir)?;

            if action_store.is_call() {
                return Ok(Some(action_store));
            }
        }

        Ok(None)
    }

    pub fn last_action(&self) -> Result<Option<ActionStore>, io::Error> {
        if self.next == 0 {
            return Ok(None);
        }

        let idx = self.next - 1;

        Ok(Some(ActionStore::from_path(&self.action_path(idx))?))
    }

    pub fn begin_call(&mut self, before: Call) -> Result<(), io::Error> {
        let idx = self.next;
        let dir = self.action_path(idx);

        fs::create_dir(&dir)?;
        serde_json::to_writer_pretty(
            BufWriter::new(
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(ActionStore::BEFORE_JSON_PATH))?,
            ),
            &before,
        )?;

        self.recording = Some(idx);

        Ok(())
    }

    pub fn end_call(&mut self, result: Call) -> Result<(), eyre::Error> {
        let idx = self.recording.take().wrap_err("not recording action")?;
        let dir = self.action_path(idx);

        serde_json::to_writer_pretty(
            BufWriter::new(
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(ActionStore::CALL_JSON_PATH))?,
            ),
            &result,
        )?;

        self.next += 1;

        Ok(())
    }

    fn action_path(&self, idx: usize) -> PathBuf {
        self.path.join(format!("{:04}", idx))
    }
}

#[derive(Clone, Debug)]
pub struct ActionStore {
    path: PathBuf,
}

impl ActionStore {
    const BEFORE_JSON_PATH: &'static str = "before.json";
    const CALL_JSON_PATH: &'static str = "call.json";
    const DECL_JSON_PATH: &'static str = "decl.json";

    pub fn from_path(path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            path: path.canonicalize()?,
        })
    }

    pub fn read(&self) -> Result<Action, eyre::Error> {
        let call_json = self.path.join(Self::CALL_JSON_PATH);

        if call_json.exists() {
            return Ok(Action::Call(
                serde_json::from_reader(BufReader::new(
                    fs::OpenOptions::new()
                        .read(true)
                        .open(&call_json)
                        .wrap_err("failed to open call.json")?,
                ))
                .wrap_err("failed to deserialize call")?,
            ));
        }

        Ok(Action::Decl(
            serde_json::from_reader(BufReader::new(
                fs::OpenOptions::new()
                    .read(true)
                    .open(self.path.join(Self::DECL_JSON_PATH))
                    .wrap_err("failed to open decl.json")?,
            ))
            .wrap_err("failed to deserialize decl")?,
        ))
    }

    pub fn is_call(&self) -> bool {
        self.path.join(Self::CALL_JSON_PATH).exists()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Call(Call),
    Decl(Decl),
}

impl Action {
    pub fn call(&self) -> Option<&Call> {
        match self {
            | Action::Call(call) => Some(&call),
            | Action::Decl(_decl) => None,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func:    String,
    pub errno:   Option<i32>,
    pub params:  Vec<ValueMeta>,
    pub results: Vec<ValueMeta>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Decl;
