use anyhow::{Result, anyhow};
use console::style;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};

#[derive(Clone, Copy)]
enum Level {
    Ok,
    Warn,
    Fail,
}

struct DoctorReport {
    ok: u32,
    warn: u32,
    fail: u32,
    hints: Vec<String>,
}

impl DoctorReport {
    fn new() -> Self {
        Self {
            ok: 0,
            warn: 0,
            fail: 0,
            hints: Vec::new(),
        }
    }

    fn line(&mut self, level: Level, message: impl AsRef<str>) {
        let msg = message.as_ref();
        match level {
            Level::Ok => {
                self.ok += 1;
                println!("{} {}", style("[OK]").green(), msg);
            }
            Level::Warn => {
                self.warn += 1;
                println!("{} {}", style("[WARN]").yellow(), msg);
            }
            Level::Fail => {
                self.fail += 1;
                println!("{} {}", style("[FAIL]").red(), msg);
            }
        }
    }

    fn hint(&mut self, hint: impl AsRef<str>) {
        let text = hint.as_ref().trim();
        if text.is_empty() {
            return;
        }
        if !self.hints.iter().any(|existing| existing == text) {
            self.hints.push(text.to_string());
        }
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn expected_link_path(base_dir: &Path, name: &str) -> PathBuf {
    let base = base_dir.join(name);
    #[cfg(windows)]
    {
        if base
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            != Some("exe".into())
        {
            return base.with_extension("exe");
        }
    }
    base
}

fn normalized_link_package_name(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        return Some(file_name.trim_end_matches(".exe").to_string());
    }
    #[cfg(not(windows))]
    {
        Some(file_name)
    }
}

fn find_stale_symlink_names(symlinks_dir: &Path, installed_names: &HashSet<String>) -> Vec<String> {
    let Ok(entries) = fs::read_dir(symlinks_dir) else {
        return Vec::new();
    };

    let mut stale = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = normalized_link_package_name(&path) else {
            continue;
        };
        if !installed_names.contains(&name) {
            stale.push(name);
        }
    }

    stale.sort();
    stale.dedup();
    stale
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
                report.line(Level::Ok, "Shell PATH integration file looks valid");
            } else {
                report.line(
                    Level::Warn,
                    "Shell PATH file does not include upstream symlinks export line",
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
}

#[cfg(not(unix))]
fn check_paths_file(_paths: &UpstreamPaths, report: &mut DoctorReport) {
    report.line(Level::Ok, "PATH integration check skipped on this platform");
}

pub fn run(names: Vec<String>) -> Result<()> {
    println!("{}", style("Running upstream doctor...").cyan());

    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let mut report = DoctorReport::new();

    for (label, path) in [
        ("data directory", paths.dirs.data_dir.as_path()),
        ("metadata directory", paths.dirs.metadata_dir.as_path()),
        (
            "symlinks directory",
            paths.integration.symlinks_dir.as_path(),
        ),
        ("icons directory", paths.integration.icons_dir.as_path()),
        ("appimages directory", paths.install.appimages_dir.as_path()),
        ("binaries directory", paths.install.binaries_dir.as_path()),
        ("archives directory", paths.install.archives_dir.as_path()),
    ] {
        if path.exists() {
            report.line(Level::Ok, format!("{} exists", label));
        } else {
            report.line(
                Level::Fail,
                format!("{} missing: {}", label, path.display()),
            );
            report.hint("Run `upstream init` to create missing upstream directories and metadata files.");
        }
    }

    if paths.config.config_file.exists() {
        report.line(Level::Ok, "Config file exists");
    } else {
        report.line(
            Level::Warn,
            format!(
                "Config file missing: {}",
                paths.config.config_file.display()
            ),
        );
        report.hint("Run `upstream init` to generate the default config file.");
    }

    if paths.config.packages_file.exists() {
        report.line(Level::Ok, "Package metadata file exists");
    } else {
        report.line(
            Level::Warn,
            format!(
                "Package metadata file missing: {}",
                paths.config.packages_file.display()
            ),
        );
        report.hint("Run `upstream init` to create package metadata storage.");
    }

    check_paths_file(&paths, &mut report);

    let all_packages = package_storage.get_all_packages();
    let installed_names: HashSet<String> = all_packages.iter().map(|p| p.name.clone()).collect();

    let stale_links = find_stale_symlink_names(&paths.integration.symlinks_dir, &installed_names);
    if stale_links.is_empty() {
        report.line(Level::Ok, "No stale symlinks detected");
    } else {
        report.line(
            Level::Warn,
            format!(
                "Detected {} stale symlink(s): {}",
                stale_links.len(),
                stale_links.join(", ")
            ),
        );
        report.hint(format!(
            "Remove stale symlinks from '{}' or run package removals with --purge.",
            paths.integration.symlinks_dir.display()
        ));
    }

    let mut selected = Vec::new();
    if names.is_empty() {
        selected.extend(all_packages.iter());
        report.line(
            Level::Ok,
            format!("Loaded {} package(s) for checks", selected.len()),
        );
    } else {
        for name in &names {
            match package_storage.get_package_by_name(name) {
                Some(pkg) => selected.push(pkg),
                None => report.line(
                    Level::Fail,
                    format!("Requested package '{}' is not installed", name),
                ),
            }
        }
        report.line(
            Level::Ok,
            format!(
                "Selected {} package(s) for checks ({} requested)",
                selected.len(),
                names.len()
            ),
        );
    }

    for package in selected {
        let package_label = format!("package '{}'", package.name);

        match &package.install_path {
            Some(path) if path.exists() => {
                report.line(Level::Ok, format!("{} install path exists", package_label));
            }
            Some(path) => {
                report.line(
                    Level::Fail,
                    format!("{} install path missing: {}", package_label, path.display()),
                );
            }
            None => {
                report.line(
                    Level::Fail,
                    format!("{} has no install path", package_label),
                );
                report.hint(format!(
                    "Package '{}' has stale metadata. Run `upstream remove {}` then reinstall.",
                    package.name, package.name
                ));
            }
        }

        match &package.exec_path {
            Some(path) if path.exists() => {
                if is_executable(path) {
                    report.line(
                        Level::Ok,
                        format!("{} executable path is valid", package_label),
                    );
                } else {
                    report.line(
                        Level::Warn,
                        format!("{} executable path is not marked executable", package_label),
                    );
                }
            }
            Some(path) => {
                report.line(
                    Level::Fail,
                    format!(
                        "{} executable path missing: {}",
                        package_label,
                        path.display()
                    ),
                );
            }
            None => {
                report.line(
                    Level::Warn,
                    format!("{} has no executable path recorded", package_label),
                );
                report.hint(format!(
                    "Try `upstream upgrade {} --force` to rebuild executable metadata.",
                    package.name
                ));
            }
        }

        if let Some(exec_path) = &package.exec_path {
            let link_path = expected_link_path(&paths.integration.symlinks_dir, &package.name);
            if link_path.exists() {
                #[cfg(unix)]
                {
                    match fs::read_link(&link_path) {
                        Ok(target) => {
                            if target == *exec_path {
                                report.line(
                                    Level::Ok,
                                    format!("{} symlink points to executable", package_label),
                                );
                            } else {
                                report.line(
                                    Level::Warn,
                                    format!(
                                        "{} symlink target differs ({} -> {})",
                                        package_label,
                                        link_path.display(),
                                        target.display()
                                    ),
                                );
                            }
                        }
                        Err(e) => report.line(
                            Level::Warn,
                            format!("{} symlink unreadable: {}", package_label, e),
                        ),
                    }
                }
                #[cfg(not(unix))]
                {
                    report.line(Level::Ok, format!("{} link entry exists", package_label));
                }
            } else {
                report.line(
                    Level::Warn,
                    format!(
                        "{} link missing in symlinks dir ({})",
                        package_label,
                        link_path.display()
                    ),
                );
                report.hint(format!(
                    "Try `upstream upgrade {} --force` to recreate missing links.",
                    package.name
                ));
            }
        }

        if let Some(icon_path) = &package.icon_path {
            if icon_path.exists() {
                report.line(Level::Ok, format!("{} icon file exists", package_label));
            } else {
                report.line(
                    Level::Warn,
                    format!(
                        "{} icon file missing: {}",
                        package_label,
                        icon_path.display()
                    ),
                );
            }

            #[cfg(unix)]
            {
                let desktop_entry = paths
                    .integration
                    .xdg_applications_dir
                    .join(format!("{}.desktop", package.name));
                if desktop_entry.exists() {
                    report.line(Level::Ok, format!("{} desktop entry exists", package_label));
                } else {
                    report.line(
                        Level::Warn,
                        format!(
                            "{} desktop entry missing: {}",
                            package_label,
                            desktop_entry.display()
                        ),
                    );
                    report.hint(format!(
                        "Reinstall '{}' with desktop integration enabled to restore desktop entry.",
                        package.name
                    ));
                }
            }
        }
    }

    println!();
    println!(
        "Doctor summary: {} OK, {} warnings, {} failures",
        report.ok, report.warn, report.fail
    );

    if !report.hints.is_empty() {
        println!();
        println!("{}", style("Suggested fixes:").cyan());
        for hint in &report.hints {
            println!(" - {}", hint);
        }
    }

    if report.fail > 0 {
        return Err(anyhow!(
            "Doctor found {} failure(s). Resolve reported issues and retry.",
            report.fail
        ));
    }

    if report.warn > 0 {
        println!(
            "{}",
            style("Doctor completed with warnings. Review the items above.").yellow()
        );
    } else {
        println!("{}", style("Doctor completed successfully.").green());
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/application/features/doctor.rs"]
mod tests;
