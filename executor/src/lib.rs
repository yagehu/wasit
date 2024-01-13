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
        let stderr_copy_handle = thread::spawn(move || {
            io::copy(&mut stderr, stderr_logger.lock().unwrap().deref_mut()).unwrap()
        });

        Ok(RunningExecutor::new(
            child,
            self.wasi_runner.base_dir_fd(),
            stderr_copy_handle,
        ))
    }
}

#[derive(Debug)]
pub struct RunningExecutor {
    child:              Arc<Mutex<process::Child>>,
    stdin:              process::ChildStdin,
    stdout:             Arc<Mutex<process::ChildStdout>>,
    stderr_copy_handle: Arc<Mutex<Option<thread::JoinHandle<u64>>>>,
    base_dir_fd:        u32,
}

impl RunningExecutor {
    pub fn new(
        child: process::Child,
        base_dir_fd: u32,
        stderr_copy_handle: thread::JoinHandle<u64>,
    ) -> Self {
        let mut child = child;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Self {
            child: Arc::new(Mutex::new(child)),
            stdin,
            stdout: Arc::new(Mutex::new(stdout)),
            stderr_copy_handle: Arc::new(Mutex::new(Some(stderr_copy_handle))),
            base_dir_fd,
        }
    }

    pub fn kill(&self) {
        self.child.lock().unwrap().kill().unwrap();
        self.stderr_copy_handle
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .join()
            .unwrap();
    }

    pub fn call(
        &self,
        call: wazzi_executor_capnp::call_request::Reader,
    ) -> Result<
        capnp::message::TypedReader<
            capnp::serialize::OwnedSegments,
            wazzi_executor_capnp::call_response::Owned,
        >,
        capnp::Error,
    > {
        let mut message = capnp::message::Builder::new_default();
        let mut request_builder = message.init_root::<wazzi_executor_capnp::request::Builder>();

        request_builder.reborrow().set_call(call)?;
        capnp::serialize::write_message(&self.stdin, &message)?;

        let message = capnp::serialize::read_message(
            self.stdout.lock().unwrap().deref_mut(),
            capnp::message::DEFAULT_READER_OPTIONS,
        )?;

        Ok(message.into_typed())
    }

    pub fn decl(
        &self,
        request: wazzi_executor_capnp::decl_request::Reader,
    ) -> Result<(), capnp::Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut request_builder = message.init_root::<wazzi_executor_capnp::request::Builder>();

        request_builder.reborrow().set_decl(request)?;
        capnp::serialize::write_message(&self.stdin, &message)?;

        let _message = capnp::serialize::read_message(
            self.stdout.lock().unwrap().deref_mut(),
            capnp::message::DEFAULT_READER_OPTIONS,
        )?;

        Ok(())
    }

    pub fn base_dir_fd(&self) -> u32 {
        self.base_dir_fd
    }
}
