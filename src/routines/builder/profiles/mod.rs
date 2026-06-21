use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result};

use crate::routines::builder::BuildProfile;

pub mod cmake;
pub mod dotnet;
pub mod go;
pub mod rust;
pub mod zig;

pub fn handlers() -> [Box<dyn BuildProfileHandler>; 5] {
    [
        Box::new(rust::RustProfile),
        Box::new(dotnet::DotnetProfile),
        Box::new(go::GoProfile),
        Box::new(zig::ZigProfile),
        Box::new(cmake::CmakeProfile),
    ]
}

pub trait BuildProfileHandler {
    fn profile(&self) -> BuildProfile;
    fn detect(&self, workspace: &Path) -> bool;
    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf>;
}

pub fn run_command_with_line_callback(
    command: &mut Command,
    context: &str,
    line_callback: &mut Option<&mut dyn FnMut(&str)>,
) -> Result<ExitStatus> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().context(context.to_string())?;
    let (tx, rx) = mpsc::channel();

    if let Some(stdout) = child.stdout.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }

    drop(tx);

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                if let Some(callback) = line_callback.as_deref_mut() {
                    callback(&line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(status) = child.try_wait()? {
                    for line in rx.try_iter() {
                        if let Some(callback) = line_callback.as_deref_mut() {
                            callback(&line);
                        }
                    }
                    return Ok(status);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return child.wait().context(context.to_string());
            }
        }
    }
}

pub fn emit_line_callback(line_callback: &mut Option<&mut dyn FnMut(&str)>, line: impl AsRef<str>) {
    if let Some(callback) = line_callback.as_deref_mut() {
        callback(line.as_ref());
    }
}
