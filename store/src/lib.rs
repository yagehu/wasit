use std::{
    collections::HashSet,
    fs,
    io::{self, BufReader, BufWriter},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use eyre::{Context, ContextCompat as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wazzi_wasi_component_model::value::ValueMeta;

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
    data:         PathBuf,
    runtimes_dir: PathBuf,
    runtimes:     HashSet<String>,
}

impl RunStore {
    pub fn resume(path: &Path) -> Result<Self, io::Error> {
        let path = path.canonicalize()?;
        let data = path.join("data");
        let runtimes_dir = path.join("runtimes");
        let mut runtimes = HashSet::new();

        for entry in fs::read_dir(&runtimes_dir)? {
            let entry = entry?;

            runtimes.insert(entry.file_name().into_string().unwrap());
        }

        Ok(Self {
            data,
            runtimes_dir,
            runtimes,
        })
    }

    pub fn new(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir(path)?;

        let path = path.canonicalize()?;
        let data = path.join("data");
        let runtimes_dir = path.join("runtimes");

        fs::create_dir(&runtimes_dir)?;

        Ok(Self {
            data,
            runtimes_dir,
            runtimes: Default::default(),
        })
    }

    pub fn new_runtime<T: Serialize + DeserializeOwned>(
        &mut self,
        name: String,
        version: &str,
    ) -> Result<RuntimeStore<T>, io::Error> {
        let store = RuntimeStore::new(&self.runtimes_dir.join(&name), version)?;

        self.runtimes.insert(name);

        Ok(store)
    }

    pub fn write_data(&self, data: &[u8]) -> Result<(), io::Error> {
        fs::write(&self.data, data)
    }

    pub fn data(&self) -> Result<Vec<u8>, io::Error> {
        fs::read(&self.data)
    }

    pub fn runtimes<'a, T: Serialize + DeserializeOwned + 'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = RuntimeStore<T>> + '_, io::Error> {
        Ok(self
            .runtimes
            .iter()
            .map(|runtime| RuntimeStore::resume(&self.runtimes_dir.join(runtime)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter())
    }
}

#[derive(Debug)]
pub struct RuntimeStore<T> {
    pub path: PathBuf,
    pub base: PathBuf,

    name:  String,
    trace: TraceStore<T>,
}

impl<T> RuntimeStore<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn resume(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;
        let base = path.join("base");
        let name = path
            .components()
            .last()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .to_string();
        let trace = TraceStore::resume(&path.join("trace"))?;

        Ok(Self {
            path,
            base,
            name,
            trace,
        })
    }

    pub fn new(path: &Path, version: &str) -> Result<Self, io::Error> {
        fs::create_dir_all(path)?;

        let path = path.canonicalize()?;
        let base = path.join("base");
        let name = path
            .components()
            .last()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .to_string();
        let version_path = path.join("version");
        let trace = TraceStore::new(&path.join("trace"))?;

        fs::create_dir_all(&base)?;
        fs::write(&version_path, version)?;

        Ok(Self {
            path,
            base,
            name,
            trace,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn trace(&self) -> &TraceStore<T> {
        &self.trace
    }

    pub fn trace_mut(&mut self) -> &mut TraceStore<T> {
        &mut self.trace
    }
}

#[derive(Debug)]
pub struct TraceStore<T> {
    path:      PathBuf,
    next:      usize,
    recording: Option<usize>,
    call:      PhantomData<T>,
}

impl<T> TraceStore<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn resume(path: &Path) -> Result<Self, io::Error> {
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
            call: PhantomData,
        })
    }

    pub fn new(path: &Path) -> Result<Self, io::Error> {
        fs::create_dir(path)?;

        let path = path.canonicalize()?;

        Ok(Self {
            path,
            next: 0,
            recording: None,
            call: PhantomData,
        })
    }

    pub fn count(&self) -> usize {
        self.next
    }

    pub fn last_call(&self) -> Result<Option<ActionStore<T>>, io::Error> {
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

    pub fn last_action(&self) -> Result<Option<ActionStore<Call>>, io::Error> {
        if self.next == 0 {
            return Ok(None);
        }

        let idx = self.next - 1;

        Ok(Some(ActionStore::from_path(&self.action_path(idx))?))
    }

    pub fn begin_call(&mut self, before: &T) -> Result<(), io::Error> {
        let idx = self.next;
        let dir = self.action_path(idx);

        fs::create_dir(&dir)?;
        serde_json::to_writer_pretty(
            BufWriter::new(
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(ActionStore::<Call>::BEFORE_JSON_PATH))?,
            ),
            &before,
        )?;

        self.recording = Some(idx);

        Ok(())
    }

    pub fn end_call(&mut self, result: &T) -> Result<(), eyre::Error> {
        let idx = self.recording.take().wrap_err("not recording action")?;
        let dir = self.action_path(idx);

        serde_json::to_writer_pretty(
            BufWriter::new(
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(ActionStore::<Call>::CALL_JSON_PATH))?,
            ),
            &result,
        )?;

        self.next += 1;

        Ok(())
    }

    fn action_path(&self, idx: usize) -> PathBuf {
        self.path.join(format!("{:04}", idx))
    }

    pub fn actions(&self) -> Result<Vec<ActionStore<Call>>, io::Error> {
        (0..self.next)
            .map(|idx| self.action_path(idx))
            .map(|path| ActionStore::from_path(&path))
            .collect::<Result<_, _>>()
    }
}

#[derive(Clone, Debug)]
pub struct ActionStore<T> {
    path: PathBuf,
    call: PhantomData<T>,
}

impl<T> ActionStore<T>
where
    T: DeserializeOwned,
{
    const BEFORE_JSON_PATH: &'static str = "before.json";
    const CALL_JSON_PATH: &'static str = "call.json";
    const DECL_JSON_PATH: &'static str = "decl.json";

    pub fn from_path(path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            path: path.canonicalize()?,
            call: PhantomData,
        })
    }

    pub fn read(&self) -> Result<T, eyre::Error> {
        let call_json = self.path.join(Self::CALL_JSON_PATH);

        if call_json.exists() {
            return serde_json::from_reader(BufReader::new(
                fs::OpenOptions::new()
                    .read(true)
                    .open(&call_json)
                    .wrap_err("failed to open call.json")?,
            ))
            .wrap_err("failed to deserialize call");
        }

        serde_json::from_reader(BufReader::new(
            fs::OpenOptions::new()
                .read(true)
                .open(self.path.join(Self::DECL_JSON_PATH))
                .wrap_err("failed to open decl.json")?,
        ))
        .wrap_err("failed to deserialize decl")
    }

    pub fn read_call(&self) -> Result<Option<T>, eyre::Error> {
        Ok(Some(self.read()?))
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
            | Action::Call(call) => Some(call),
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
