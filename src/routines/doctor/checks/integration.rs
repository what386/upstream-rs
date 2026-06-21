#[cfg(unix)]
use crate::services::integration::{ShellManager, nushell_paths_file_contains_path};
use crate::{
    services::integration::{
        CompletionCacheMismatch, CompletionCacheMismatchKind, CompletionManager,
    },
    utils::static_paths::UpstreamPaths,
};
#[cfg(unix)]
use std::fs;

use super::super::{DoctorReport, Level};

fn completion_cache_mismatch_message(
    package_name: &str,
    mismatch: &CompletionCacheMismatch,
) -> String {
    match mismatch.kind {
        CompletionCacheMismatchKind::Missing => format!(
            "package '{}' cached {} completion is missing from shell directory: {} (cache: {})",
            package_name,
            mismatch.shell.label(),
            mismatch.installed_path.display(),
            mismatch.cached_path.display()
        ),
        CompletionCacheMismatchKind::Different => format!(
            "package '{}' cached {} completion differs from shell completion: {} (cache: {})",
            package_name,
            mismatch.shell.label(),
            mismatch.installed_path.display(),
            mismatch.cached_path.display()
        ),
    }
}

pub(in crate::routines::doctor::checks) fn check_completion_cache_drift(
    completion_manager: &CompletionManager<'_>,
    package_name: &str,
    fix: bool,
    report: &mut DoctorReport,
) {
    let mismatches = match completion_manager.cached_completion_mismatches(package_name) {
        Ok(mismatches) => mismatches,
        Err(err) => {
            report.line(
                Level::Warn,
                format!(
                    "package '{}' cached completion check failed: {}",
                    package_name, err
                ),
            );
            return;
        }
    };

    if mismatches.is_empty() {
        return;
    }

    for mismatch in &mismatches {
        report.line(
            Level::Warn,
            completion_cache_mismatch_message(package_name, mismatch),
        );
    }
    report.hint("Run `upstream doctor --fix [package]` to copy cached completions into shell completion directories.");

    if !fix {
        return;
    }

    let mut no_messages: Option<fn(&str)> = None;
    match completion_manager.copy_cached_completions_to_shells(package_name, &mut no_messages) {
        Ok(0) => report.line(
            Level::Warn,
            format!(
                "package '{}' has cached completion drift, but no cached completions were copied",
                package_name
            ),
        ),
        Ok(count) => report.line(
            Level::Ok,
            format!(
                "package '{}' copied {} cached completion(s) to shell directories",
                package_name, count
            ),
        ),
        Err(err) => report.line(
            Level::Warn,
            format!(
                "package '{}' failed to copy cached completions during fix: {}",
                package_name, err
            ),
        ),
    }
}

pub(in crate::routines::doctor) fn check_completion_directories<'a>(
    paths: &'a UpstreamPaths,
    report: &mut DoctorReport,
) -> CompletionManager<'a> {
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

    completion_manager
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::services::integration::{
        CompletionCacheMismatch, CompletionCacheMismatchKind, CompletionShell,
    };

    use super::completion_cache_mismatch_message;

    #[test]
    fn completion_cache_mismatch_warning_names_cache_and_shell_paths() {
        let mismatch = CompletionCacheMismatch {
            shell: CompletionShell::Fish,
            cached_path: PathBuf::from("/upstream/cache/completions/rg/rg.fish"),
            installed_path: PathBuf::from("/fish/completions/rg.fish"),
            kind: CompletionCacheMismatchKind::Different,
        };

        let message = completion_cache_mismatch_message("rg", &mismatch);

        assert!(message.contains("package 'rg'"));
        assert!(message.contains("cached fish completion differs"));
        assert!(message.contains("/fish/completions/rg.fish"));
        assert!(message.contains("/upstream/cache/completions/rg/rg.fish"));
    }
}
