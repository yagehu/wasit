use std::{
    ffi::OsString,
    fmt,
    fs,
    io,
    path::{Path, PathBuf},
    process,
};

use eyre::Context;
use tera::Tera;

pub trait WasiRunner: fmt::Debug + Send + Sync {
    fn base_dir_fd(&self) -> u32;
    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command;

    fn run(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> Result<process::Child, eyre::Error> {
        let mut command = self.prepare_command(wasm_path, working_dir, base_dir);
        let child = command
            .spawn()
            .wrap_err(format!("failed to spawn command {:?}", command))?;

        Ok(child)
    }
}

#[derive(Clone, Debug)]
pub struct Node<'p> {
    path: &'p Path,
}

impl<'p> Node<'p> {
    pub fn new(path: &'p Path) -> Self {
        Self { path }
    }
}

impl WasiRunner for Node<'_> {
    fn base_dir_fd(&self) -> u32 {
        3
    }

    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command {
        static GLUE_TMPL: &str = include_str!("run.js.tera.tmpl");

        let mut tmpl_ctx = tera::Context::new();

        tmpl_ctx.insert("executor", &wasm_path.canonicalize().unwrap());

        if let Some(base_dir) = base_dir {
            tmpl_ctx.insert("execroot", &base_dir.canonicalize().unwrap());
        }

        let glue = Tera::one_off(GLUE_TMPL, &tmpl_ctx, false).unwrap();
        let glue_path = working_dir.join("glue.js");

        fs::write(&glue_path, glue).unwrap();

        let mut command = process::Command::new(self.path);

        command.arg(glue_path);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());
        command.current_dir(working_dir);

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wasmedge<'p> {
    path: &'p Path,
}

impl<'p> Wasmedge<'p> {
    pub fn new(path: &'p Path) -> Self {
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

    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());
        command.current_dir(working_dir);

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wasmer<'p> {
    path: &'p Path,
}

impl<'p> Wasmer<'p> {
    pub fn new(path: &'p Path) -> Self {
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

    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());
        command.current_dir(working_dir);

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wasmtime<'p> {
    path: &'p Path,
}

impl<'p> Wasmtime<'p> {
    pub fn new(path: &'p Path) -> Self {
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

    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![OsString::from("run")];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());
        command.current_dir(working_dir);

        command
    }
}

#[derive(Clone, Debug)]
pub struct Wamr<'p> {
    path: &'p Path,
}

impl<'p> Wamr<'p> {
    pub fn new(path: &'p Path) -> Self {
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

    fn prepare_command(
        &self,
        wasm_path: PathBuf,
        working_dir: &Path,
        base_dir: Option<PathBuf>,
    ) -> process::Command {
        let mut command = process::Command::new(self.path);
        let mut args = vec![];

        args.extend(self.mount_base_dir(base_dir));
        args.push(wasm_path.into());

        command.args(args);
        command.stdin(process::Stdio::piped());
        command.stdout(process::Stdio::piped());
        command.stderr(process::Stdio::piped());
        command.current_dir(working_dir);

        command
    }
}
