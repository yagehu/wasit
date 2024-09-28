extern crate wazzi_executor_pb_rust as pb;

use std::{
    ffi::OsString,
    fmt,
    fs,
    io,
    ops::DerefMut as _,
    path::{Path, PathBuf},
    process,
    sync::{Arc, Mutex},
    thread,
};

use eyre::Context;
use protobuf::Message as _;
use serde::{Deserialize, Serialize};
use tera::Tera;

#[derive(Clone, Debug)]
pub struct RunningExecutor {
    stdin:  Arc<Mutex<process::ChildStdin>>,
    stdout: Arc<Mutex<process::ChildStdout>>,
}

impl RunningExecutor {
    pub fn from_wasi_runner<W>(
        wasi_runner: &dyn WasiRunner,
        executor_bin: &Path,
        working_dir: &Path,
        stderr_logger: Arc<Mutex<W>>,
        preopens: Vec<MappedDir>,
    ) -> Result<Self, eyre::Error>
    where
        W: io::Write + Send + 'static,
    {
        let mut child = wasi_runner
            .run(executor_bin, working_dir, preopens)
            .wrap_err(format!("failed to run executor {}", executor_bin.display()))?;
        let mut stderr = child.stderr.take().unwrap();
        let _stderr_copy_handle = thread::spawn(move || {
            io::copy(&mut stderr, stderr_logger.lock().unwrap().deref_mut()).unwrap()
        });
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Ok(Self {
            stdin:  Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
        })
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
}

pub trait WasiRunner: fmt::Debug + Send + Sync {
    fn run(
        &self,
        wasm_path: &Path,
        working_dir: &Path,
        preopens: Vec<MappedDir>,
    ) -> Result<process::Child, eyre::Error>;
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Node<'p> {
    path: &'p Path,
}

impl<'p> Node<'p> {
    pub fn new(path: &'p Path) -> Self {
        Self { path }
    }
}

impl Default for Node<'_> {
    fn default() -> Self {
        Self::new(Path::new("node"))
    }
}

impl WasiRunner for Node<'_> {
    fn run(
        &self,
        wasm_path: &Path,
        working_dir: &Path,
        preopens: Vec<MappedDir>,
    ) -> Result<process::Child, eyre::Error> {
        static GLUE_TMPL: &str = include_str!("run.js.tera.tmpl");

        let mut tmpl_ctx = tera::Context::new();

        tmpl_ctx.insert("executor", &wasm_path.canonicalize().unwrap());
        tmpl_ctx.insert("preopens", &preopens);

        let glue = Tera::one_off(GLUE_TMPL, &tmpl_ctx, false).unwrap();
        let glue_path = working_dir.join("glue.js");

        fs::write(&glue_path, glue).unwrap();

        let mut command = process::Command::new(self.path);

        command
            .arg(glue_path)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .current_dir(working_dir)
            .spawn()
            .wrap_err("failed to spawn command")
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Wasmedge<'p> {
    path: &'p Path,
}

impl<'p> Wasmedge<'p> {
    pub fn new(path: &'p Path) -> Self {
        Self { path }
    }
}

impl Default for Wasmedge<'_> {
    fn default() -> Self {
        Self::new(Path::new("wasmedge"))
    }
}

impl WasiRunner for Wasmedge<'_> {
    fn run(
        &self,
        wasm_path: &Path,
        working_dir: &Path,
        preopens: Vec<MappedDir>,
    ) -> Result<process::Child, eyre::Error> {
        let mut command = process::Command::new(self.path);

        command.arg("run");

        for dir in preopens {
            let mut dir_arg = OsString::new();

            dir_arg.push(dir.name);
            dir_arg.push(":");
            dir_arg.push(dir.host_path);
            command.arg("--dir");
            command.arg(dir_arg);
        }

        command
            .arg(wasm_path)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .current_dir(working_dir)
            .spawn()
            .wrap_err("failed to spawn command")
    }
}

#[derive(Clone, Debug)]
pub struct Wasmer<'p> {
    pub path: &'p Path,
}

impl Default for Wasmer<'_> {
    fn default() -> Self {
        Self::new(Path::new("wasmer"))
    }
}

impl<'p> Wasmer<'p> {
    pub fn new(path: &'p Path) -> Self {
        Self { path }
    }
}

impl WasiRunner for Wasmer<'_> {
    fn run(
        &self,
        wasm_path: &Path,
        working_dir: &Path,
        preopens: Vec<MappedDir>,
    ) -> Result<process::Child, eyre::Error> {
        let mut command = process::Command::new(self.path);

        command.arg("run");

        for dir in preopens {
            let mut mapdir = OsString::new();

            mapdir.push("/");
            mapdir.push(&dir.name);
            mapdir.push(":");
            mapdir.push(&dir.host_path);
            command.arg("--mapdir").arg(mapdir);
        }

        command
            .arg(wasm_path)
            .current_dir(working_dir)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()
            .wrap_err("failed to spawn command")
    }
}

#[derive(Clone, Debug)]
pub struct Wamr<'p> {
    path: &'p Path,
}

impl Default for Wamr<'_> {
    fn default() -> Self {
        Self::new(Path::new("iwasm"))
    }
}

impl<'p> Wamr<'p> {
    pub fn new(path: &'p Path) -> Self {
        Self { path }
    }
}

impl WasiRunner for Wamr<'_> {
    fn run(
        &self,
        wasm_path: &Path,
        working_dir: &Path,
        preopens: Vec<MappedDir>,
    ) -> Result<process::Child, eyre::Error> {
        let mut command = process::Command::new(self.path);

        for dir in preopens {
            let mut dir_arg = OsString::new();

            dir_arg.push("--dir=");
            dir_arg.push(&dir.host_path);
            command.arg(dir_arg);
        }

        command
            .arg("--stack-size=1000000")
            .arg(wasm_path)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .current_dir(working_dir)
            .spawn()
            .wrap_err("failed to spawn command")
    }
}

// #[derive(Clone, Debug)]
// pub struct Wazero<'p> {
//     path: &'p Path,
// }

// impl<'p> Wazero<'p> {
//     pub fn new(path: &'p Path) -> Self {
//         Self { path }
//     }

//     fn mount_base_dir(&self, dir: Option<PathBuf>) -> Vec<OsString> {
//         match dir {
//             | Some(dir) => vec![OsString::from("-mount"), dir.into()],
//             | None => Vec::new(),
//         }
//     }
// }

// impl WasiRunner for Wazero<'_> {
//     fn base_dir_fd(&self) -> u32 {
//         3
//     }

//     fn prepare_command(
//         &self,
//         wasm_path: PathBuf,
//         working_dir: &Path,
//         base_dir: Option<PathBuf>,
//     ) -> (process::Command, Option<Vec<u8>>) {
//         let mut command = process::Command::new(self.path);

//         command.arg("run");
//         command.args(self.mount_base_dir(base_dir));
//         command.arg(wasm_path);
//         command.stdin(process::Stdio::piped());
//         command.stdout(process::Stdio::piped());
//         command.stderr(process::Stdio::piped());
//         command.current_dir(working_dir);

//         (command, None)
//     }
// }

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct MappedDir {
    pub name:      String,
    pub host_path: PathBuf,
}
