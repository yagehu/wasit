extern crate wazzi_executor_pb_rust as pb;

use std::{
    io,
    ops::DerefMut,
    path::PathBuf,
    process,
    sync::{Arc, Mutex},
    thread,
};

use protobuf::Message;
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
    stdin:              Arc<Mutex<process::ChildStdin>>,
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
            stdin: Arc::new(Mutex::new(stdin)),
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

    pub fn call(&self, call: pb::request::Call) -> Result<pb::response::Call, protobuf::Error> {
        let mut stdin = self.stdin.lock().unwrap();
        let mut stdout = self.stdout.lock().unwrap();
        let mut os = protobuf::CodedOutputStream::new(stdin.deref_mut());
        let mut is = protobuf::CodedInputStream::new(stdout.deref_mut());
        let mut request = pb::Request::new();

        request.set_call(call);

        let message_size = request.compute_size();

        os.write_raw_bytes(&message_size.to_le_bytes()).unwrap();
        request.write_to(&mut os)?;
        drop(os);

        let msg_size = is.read_fixed64()?;
        let raw_bytes = is.read_raw_bytes(msg_size as u32)?;

        Ok(pb::Response::parse_from_bytes(&raw_bytes)?.take_call())
    }

    pub fn base_dir_fd(&self) -> u32 {
        self.base_dir_fd
    }
}
