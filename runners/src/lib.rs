use std::{
    ffi::OsString,
    io::{self},
    path::PathBuf,
    process,
};

pub trait WasiRunner {
    fn base_dir_fd(&self) -> u32;
    fn prepare_command(&self, wasm_path: PathBuf, base_dir: Option<PathBuf>) -> process::Command;

    #[tracing::instrument(skip(self))]
    fn run(
        &self,
        wasm_path: PathBuf,
        base_dir: Option<PathBuf>,
    ) -> Result<process::Child, io::Error> {
        let mut command = self.prepare_command(wasm_path, base_dir);
        let child = command.spawn()?;

        Ok(child)
    }
}

#[derive(Clone, Debug)]
pub struct Wasmedge<'p> {
    path: &'p str,
}

impl<'p> Wasmedge<'p> {
    pub fn new(path: &'p str) -> Self {
        Self { path }
    }

    fn mount_base_dir(&self, dir: Option<PathBuf>) -> Vec<OsString> {
        match dir {
            | Some(dir) => vec!["--dir".into(), dir.into()],
            | None => Vec::new(),
        }
    }
}

impl WasiRunner for Wasmedge<'_> {
    fn base_dir_fd(&self) -> u32 {
        3
    }

    fn prepare_command(&self, wasm_path: PathBuf, base_dir: Option<PathBuf>) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wasmer<'p> {
    path: &'p str,
}

impl<'p> Wasmer<'p> {
    pub fn new(path: &'p str) -> Self {
        Self { path }
    }

    fn mount_base_dir(&self, dir: Option<PathBuf>) -> Vec<OsString> {
        match dir {
            | Some(dir) => vec!["--mapdir".into(), format!(".:{}", dir.display()).into()],
            | None => Vec::new(),
        }
    }
}

impl WasiRunner for Wasmer<'_> {
    fn base_dir_fd(&self) -> u32 {
        4
    }

    fn prepare_command(&self, wasm_path: PathBuf, base_dir: Option<PathBuf>) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wasmtime<'p> {
    path: &'p str,
}

impl<'p> Wasmtime<'p> {
    pub fn new(path: &'p str) -> Self {
        Self { path }
    }

    fn mount_base_dir(&self, dir: Option<PathBuf>) -> Vec<OsString> {
        match dir {
            | Some(dir) => vec!["--dir".into(), dir.into()],
            | None => Vec::new(),
        }
    }
}

impl WasiRunner for Wasmtime<'_> {
    fn base_dir_fd(&self) -> u32 {
        3
    }

    fn prepare_command(&self, wasm_path: PathBuf, base_dir: Option<PathBuf>) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wamr<'p> {
    path: &'p str,
}

impl<'p> Wamr<'p> {
    pub fn new(path: &'p str) -> Self {
        Self { path }
    }

    fn mount_base_dir(&self, dir: Option<PathBuf>) -> Vec<OsString> {
        match dir {
            | Some(dir) => vec![OsString::from(format!("--dir={}", dir.display()))],
            | None => Vec::new(),
        }
    }
}

impl WasiRunner for Wamr<'_> {
    fn base_dir_fd(&self) -> u32 {
        3
    }

    fn prepare_command(&self, wasm_path: PathBuf, base_dir: Option<PathBuf>) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());

        command
    }
}
