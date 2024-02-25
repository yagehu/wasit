pub mod parse;

use std::{io, path::Path, process};

pub struct Strace {
    process: process::Child,
}

impl Strace {
    pub fn attach(pid: u32, output_file_path: &Path) -> Result<Self, io::Error> {
        let child = process::Command::new("strace")
            .args(["--no-abbrev", "--string-limit", "1024"])
            .arg("--attach")
            .arg(format!("{pid}"))
            .arg("--output")
            .arg(output_file_path)
            .spawn()?;

        Ok(Self { process: child })
    }

    pub fn stop(mut self) -> Result<(), io::Error> {
        self.process.kill()
    }
}
