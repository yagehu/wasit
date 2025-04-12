use std::{
    collections::HashMap,
    fs,
    io::{self, BufWriter},
    path::{Path, PathBuf},
    sync::{
        atomic::{self, AtomicUsize},
        Arc,
        RwLock,
    },
};

use serde::{de::DeserializeOwned, Serialize};
use tracing_subscriber::layer::SubscriberExt as _;

#[derive(Serialize, Debug)]
pub struct FuzzMetadata {
    pub ncalls: usize,
}

#[derive(Debug)]
pub struct Store {
    path:   PathBuf,
    next:   Arc<AtomicUsize>,
    ncalls: Arc<AtomicUsize>,
}

impl Store {
    pub fn new(path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            path:   path.canonicalize()?,
            next:   Arc::new(AtomicUsize::new(0)),
            ncalls: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn new_run<T>(&self) -> Result<(String, RunStore<T>), io::Error> {
        let idx = self.next.fetch_add(1, atomic::Ordering::AcqRel);
        let id = format!("{idx}");
        let path = self.path.join("runs").join(&id);

        fs::create_dir_all(&path)?;
        fs::create_dir(path.join("data"))?;
        fs::create_dir(path.join("runtimes"))?;

        Ok((
            id,
            RunStore {
                path:           path.clone(),
                data_next_idx:  0,
                data_dir:       path.join("data"),
                runtimes_dir:   path.canonicalize()?.join("runtimes"),
                runtimes:       Default::default(),
                tracing_guards: Vec::new(),
                total_ncalls:   self.ncalls.clone(),
            },
        ))
    }

    pub fn root_path(&self) -> &Path {
        &self.path
    }

    pub fn metadata(&self) -> FuzzMetadata {
        FuzzMetadata {
            ncalls: self.ncalls.load(atomic::Ordering::Acquire),
        }
    }
}

#[derive(Debug)]
pub struct RunStore<T> {
    path:           PathBuf,
    data_next_idx:  usize,
    data_dir:       PathBuf,
    tracing_guards: Vec<tracing::dispatcher::DefaultGuard>,
    runtimes_dir:   PathBuf,
    runtimes:       HashMap<String, Arc<RwLock<RuntimeStore<T>>>>,
    total_ncalls:   Arc<AtomicUsize>,
}

impl<T: Serialize + DeserializeOwned> RunStore<T> {
    pub fn new_runtime(
        &mut self,
        name: String,
        enable_logging: bool,
    ) -> Result<Arc<RwLock<RuntimeStore<T>>>, io::Error> {
        let store = Arc::new(RwLock::new(RuntimeStore::new(
            &self.runtimes_dir.join(&name),
            enable_logging,
        )?));

        self.runtimes.insert(name, store.clone());

        Ok(store)
    }

    pub fn finish(&mut self) {
        let mut runtimes = self.runtimes.iter();
        let (rt0_name, rt0) = runtimes.next().unwrap();
        let rt0_ncalls = rt0.read().unwrap().next_call_idx;

        for (rt1_name, rt1) in runtimes {
            let rt1_ncalls = rt1.read().unwrap().next_call_idx;

            if rt0_ncalls != rt1_ncalls {
                panic!("number of calls mistach {rt0_name} {rt1_name}");
            }
        }

        self.total_ncalls.fetch_add(rt0_ncalls, atomic::Ordering::AcqRel);
    }

    /// Should be called only once per thread.
    pub fn configure_progress_logging(&mut self, enable: bool) {
        if enable {
            let progress_file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(self.path.join("progress"))
                .unwrap();
            let fuzzer_tracing_subscriber = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_thread_names(true)
                .with_writer(progress_file);
            let tracing_guard = tracing::subscriber::set_default(
                tracing_subscriber::Registry::default().with(fuzzer_tracing_subscriber),
            );

            self.tracing_guards.push(tracing_guard);
        }
    }

    pub fn write_data(&mut self, data: &[u8]) -> Result<(), io::Error> {
        tracing::debug!("Recording newly filled buffer.",);

        fs::write(&self.data_dir.join(format!("{}", self.data_next_idx)), data)?;
        self.data_next_idx += 1;

        Ok(())
    }

    pub fn runtime_stores(&self) -> impl Iterator<Item = (&str, Arc<RwLock<RuntimeStore<T>>>)> + use<'_, T> {
        self.runtimes.iter().map(|(name, store)| (name.as_str(), store.clone()))
    }
}

#[derive(Debug)]
pub struct RuntimeStore<T> {
    root_path:     PathBuf,
    base_path:     PathBuf,
    log_trace:     Option<PathBuf>,
    next_call_idx: usize,
    last_call:     Option<T>,
}

impl<T> RuntimeStore<T> {
    pub fn new(path: &Path, log_trace: bool) -> Result<Self, io::Error> {
        fs::create_dir(path)?;
        fs::create_dir(&path.join("base"))?;

        let root_path = path.canonicalize()?;
        let base_path = root_path.join("base");
        let log_trace = match log_trace {
            | true => {
                let trace_path = root_path.join("trace");

                fs::create_dir(&trace_path)?;

                Some(trace_path)
            },
            | false => None,
        };

        Ok(Self {
            root_path,
            base_path,
            log_trace,
            next_call_idx: 0,
            last_call: None,
        })
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }
}

impl<T> RuntimeStore<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn last_call(&self) -> Option<&T> {
        self.last_call.as_ref()
    }

    pub fn record_call(&mut self, result: T) -> Result<(), eyre::Error> {
        if let Some(trace_path) = &self.log_trace {
            serde_json::to_writer_pretty(
                BufWriter::new(
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(trace_path.join(format!("{:04}.json", self.next_call_idx)))?,
                ),
                &result,
            )?;
        }

        self.next_call_idx += 1;
        self.last_call = Some(result);

        Ok(())
    }
}
