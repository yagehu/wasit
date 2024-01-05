pub mod wazzi_executor_capnp {
    include!(concat!(env!("OUT_DIR"), "/wazzi_executor_capnp.rs"));
}

use std::{
    io,
    ops::DerefMut,
    path::PathBuf,
    process,
    sync::{Arc, Mutex},
    thread,
};

use wazzi_runners::WasiRunner;

#[derive(Debug)]
pub struct ExecutorRunner<WR> {
    wasi_runner: WR,
    executor:    PathBuf,
    base_dir:    Option<PathBuf>,
}

impl<WR> ExecutorRunner<WR>
where
    WR: WasiRunner,
{
    pub fn new(wasi_runner: WR, executor: PathBuf, base_dir: Option<PathBuf>) -> Self {
        Self {
            wasi_runner,
            executor,
            base_dir,
        }
    }

    pub fn run<W>(&self, stderr_logger: Arc<Mutex<W>>) -> Result<RunningExecutor, io::Error>
    where
        W: io::Write + Send + 'static,
    {
        let mut child = self
            .wasi_runner
            .run(self.executor.clone(), self.base_dir.clone())?;
        let mut stderr = child.stderr.take().unwrap();

        thread::spawn(move || io::copy(&mut stderr, stderr_logger.lock().unwrap().deref_mut()));

        Ok(RunningExecutor::new(child))
    }
}

#[derive(Debug)]
pub struct RunningExecutor {
    child:  process::Child,
    stdin:  process::ChildStdin,
    stdout: process::ChildStdout,
}

impl RunningExecutor {
    pub fn new(child: process::Child) -> Self {
        let mut child = child;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Self {
            child,
            stdin,
            stdout,
        }
    }

    pub fn call(&mut self, call: wazzi_executor_capnp::call::Reader) -> Result<(), capnp::Error> {
        let mut message = capnp::message::Builder::new_default();

        message.set_root::<wazzi_executor_capnp::call::Reader>(call)?;
        capnp::serialize::write_message(&self.stdin, &message)?;

        let message = capnp::serialize::read_message(
            &mut self.stdout,
            capnp::message::DEFAULT_READER_OPTIONS,
        )?;

        Ok(())
    }
}
