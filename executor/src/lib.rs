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

        Ok(RunningExecutor::new(child, self.wasi_runner.base_dir_fd()))
    }
}

#[derive(Debug)]
pub struct RunningExecutor {
    child:       process::Child,
    stdin:       process::ChildStdin,
    stdout:      process::ChildStdout,
    base_dir_fd: u32,
}

impl RunningExecutor {
    pub fn new(child: process::Child, base_dir_fd: u32) -> Self {
        let mut child = child;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Self {
            child,
            stdin,
            stdout,
            base_dir_fd,
        }
    }

    pub fn call(
        &mut self,
        call: wazzi_executor_capnp::call_request::Reader,
    ) -> Result<(), capnp::Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut request_builder = message.init_root::<wazzi_executor_capnp::request::Builder>();

        request_builder.reborrow().set_call(call)?;
        capnp::serialize::write_message(&self.stdin, &message)?;

        let message = capnp::serialize::read_message(
            &mut self.stdout,
            capnp::message::DEFAULT_READER_OPTIONS,
        )?;

        Ok(())
    }

    pub fn decl(
        &mut self,
        request: wazzi_executor_capnp::decl_request::Reader,
    ) -> Result<(), capnp::Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut request_builder = message.init_root::<wazzi_executor_capnp::request::Builder>();

        request_builder.reborrow().set_decl(request)?;
        capnp::serialize::write_message(&self.stdin, &message)?;

        let message = capnp::serialize::read_message(
            &mut self.stdout,
            capnp::message::DEFAULT_READER_OPTIONS,
        )?;

        Ok(())
    }

    pub fn base_dir_fd(&self) -> u32 {
        self.base_dir_fd
    }
}
