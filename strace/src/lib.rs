pub mod parse;

use std::{
    fs,
    io::{self, BufRead, BufReader},
    path::Path,
    process,
    thread,
};

use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

pub struct Strace {
    process: process::Child,
}

impl Strace {
    pub fn attach(pid: u32, output_file_path: &Path) -> Result<Self, io::Error> {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_file_path)?;

        let mut child = process::Command::new("strace")
            .args(["--no-abbrev", "--string-limit", "0", "--follow-forks"])
            .arg("--attach")
            .arg(format!("{pid}"))
            .arg("--output")
            .arg(output_file_path)
            .stderr(process::Stdio::piped())
            .spawn()?;
        let stderr = child.stderr.take().unwrap();
        let mut stderr = BufReader::new(stderr);
        let mut buf = Vec::new();

        stderr.read_until(b'\n', &mut buf)?;
        std::thread::sleep(std::time::Duration::from_secs(1));

        thread::spawn(move || io::copy(&mut stderr, &mut io::stderr()).unwrap());

        Ok(Self { process: child })
    }

    pub fn stop(mut self) -> Result<(), io::Error> {
        std::thread::sleep(std::time::Duration::from_secs(1));
        signal::kill(Pid::from_raw(self.process.id() as i32), Signal::SIGTERM)?;
        self.process.wait()?;

        Ok(())
    }
}
