#[cfg(unix)]
use crate::services::integration::{ShellManager, nushell_paths_file_contains_path};
use crate::{services::integration::CompletionManager, utils::static_paths::UpstreamPaths};
#[cfg(unix)]
use std::fs;

use super::super::{DoctorReport, Level};

pub(in crate::routines::doctor) fn check_completion_directories(
    paths: &UpstreamPaths,
    report: &mut DoctorReport,
) {
    let completion_manager = CompletionManager::new(paths);
    let completion_dirs = completion_manager.installed_shell_completion_dirs();
    if completion_dirs.is_empty() {
        report.line(
            Level::Ok,
            "No supported shells detected for completion checks",
        );
    }
    for (shell, path) in completion_dirs {
        if path.exists() {
            report.line(Level::Ok, format!("{shell} completions directory exists"));
        } else {
            report.line(
                Level::Fail,
                format!("{shell} completions directory missing: {}", path.display()),
            );
            report.hint(
                "Run `upstream hooks init` to create completion directories for installed shells.",
            );
        }
    }
}

pub(in crate::routines::doctor) fn check_path_integration(
    paths: &UpstreamPaths,
    fix: bool,
    report: &mut DoctorReport,
) {
    check_paths_file(paths, report);
    if fix {
        fix_paths_file(paths, report);
    }
}

#[cfg(unix)]
fn check_paths_file(paths: &UpstreamPaths, report: &mut DoctorReport) {
    if !paths.config.paths_file.exists() {
        report.line(
            Level::Warn,
            format!("PATH file missing: {}", paths.config.paths_file.display()),
        );
        return;
    }

    let expected_line = format!(
        "export PATH=\"{}:$PATH\"",
        paths.integration.symlinks_dir.display()
    );

    match fs::read_to_string(&paths.config.paths_file) {
        Ok(content) => {
            if content.contains(&expected_line) {
                report.line(Level::Ok, "POSIX shell PATH integration file looks valid");
            } else {
                report.line(
                    Level::Warn,
                    "POSIX shell PATH file does not include upstream symlinks export line",
                );
            }
        }
        Err(e) => report.line(
            Level::Warn,
            format!(
                "Failed to read PATH integration file '{}': {}",
                paths.config.paths_file.display(),
                e
            ),
        ),
    }

    if !paths.config.paths_nu_file.exists() {
        report.line(
            Level::Warn,
            format!(
                "Nushell PATH file missing: {}",
                paths.config.paths_nu_file.display()
            ),
        );
        return;
    }

    let expected_nushell_path = paths.integration.symlinks_dir.display().to_string();

    match fs::read_to_string(&paths.config.paths_nu_file) {
        Ok(content) => {
            if nushell_paths_file_contains_path(&content, &expected_nushell_path) {
                report.line(Level::Ok, "Nushell PATH integration file looks valid");
            } else {
                report.line(
                    Level::Warn,
                    "Nushell PATH file does not include upstream symlinks path",
                );
            }
        }
        Err(e) => report.line(
            Level::Warn,
            format!(
                "Failed to read Nushell PATH integration file '{}': {}",
                paths.config.paths_nu_file.display(),
                e
            ),
        ),
    }
}

#[cfg(unix)]
fn fix_paths_file(paths: &UpstreamPaths, report: &mut DoctorReport) {
    let manager = ShellManager::new(&paths.config.paths_file);
    if let Err(err) = manager.add_to_paths(&paths.integration.symlinks_dir) {
        report.line(
            Level::Warn,
            format!("Failed to repair PATH integration file: {}", err),
        );
        return;
    }
    report.line(Level::Ok, "Repaired PATH integration file");
}

#[cfg(not(unix))]
fn fix_paths_file(_paths: &UpstreamPaths, _report: &mut DoctorReport) {}

#[cfg(not(unix))]
fn check_paths_file(_paths: &UpstreamPaths, report: &mut DoctorReport) {
    report.line(Level::Ok, "PATH integration check skipped on this platform");
}
