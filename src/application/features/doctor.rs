use anyhow::{Result, anyhow};
use console::style;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    services::integration::{SymlinkManager, permission_handler},
    services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
#[cfg(unix)]
use crate::services::integration::ShellManager;

#[derive(Clone, Copy)]
enum Level {
    Ok,
    Warn,
    Fail,
}

struct DoctorReport {
    verbose: bool,
    ok: u32,
    warn: u32,
    fail: u32,
    warnings: Vec<String>,
    failures: Vec<String>,
    hints: Vec<String>,
}

impl DoctorReport {
    fn new(verbose: bool) -> Self {
        Self {
            verbose,
            ok: 0,
            warn: 0,
            fail: 0,
            warnings: Vec::new(),
            failures: Vec::new(),
            hints: Vec::new(),
        }
    }

    fn line(&mut self, level: Level, message: impl AsRef<str>) {
        let msg = message.as_ref();
        match level {
            Level::Ok => {
                self.ok += 1;
                if self.verbose {
                    println!("{} {}", style("[OK]").green(), msg);
                }
            }
            Level::Warn => {
                self.warn += 1;
                self.warnings.push(msg.to_string());
                if self.verbose {
                    println!("{} {}", style("[WARN]").yellow(), msg);
                }
            }
            Level::Fail => {
                self.fail += 1;
                self.failures.push(msg.to_string());
                if self.verbose {
                    println!("{} {}", style("[FAIL]").red(), msg);
                }
            }
        }
    }

    fn total_checks(&self) -> u32 {
        self.ok + self.warn + self.fail
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

    fn print_summary(&self) {
        println!("{}/{} checks ok", self.ok, self.total_checks());
        if !self.warnings.is_empty() {
            println!();
            println!("{}", style("warnings:").yellow());
            for warning in &self.warnings {
                println!(" - {}", warning);
            }
        }

        if !self.failures.is_empty() {
            println!();
            println!("{}", style("failures:").red());
            for failure in &self.failures {
                println!(" - {}", failure);
            }
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
        let name = file_name
            .strip_suffix(".exe")
            .or_else(|| file_name.strip_suffix(".EXE"))
            .unwrap_or(&file_name);
        return Some(name.to_string());
    }
    #[cfg(not(windows))]
    {
        Some(file_name)
    }
}

#[cfg(unix)]
enum LinkStatus {
    Missing,
    Unreadable(String),
    NotSymlink,
    Target {
        raw_target: PathBuf,
        resolved_target: PathBuf,
        exists: bool,
        matches_expected: bool,
    },
}

#[cfg(unix)]
fn inspect_unix_link(link_path: &Path, expected_target: &Path) -> LinkStatus {
    let metadata = match fs::symlink_metadata(link_path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return LinkStatus::Missing,
        Err(err) => return LinkStatus::Unreadable(err.to_string()),
    };

    if !metadata.file_type().is_symlink() {
        return LinkStatus::NotSymlink;
    }

    match fs::read_link(link_path) {
        Ok(raw_target) => {
            let resolved_target = if raw_target.is_absolute() {
                raw_target.clone()
            } else {
                link_path
                    .parent()
                    .map(|parent| parent.join(&raw_target))
                    .unwrap_or_else(|| raw_target.clone())
            };

            LinkStatus::Target {
                raw_target,
                exists: resolved_target.exists(),
                matches_expected: resolved_target == expected_target,
                resolved_target,
            }
        }
        Err(err) => LinkStatus::Unreadable(err.to_string()),
    }
}

fn find_stale_symlink_names(symlinks_dir: &Path, installed_names: &HashSet<String>) -> Vec<String> {
    let Ok(entries) = fs::read_dir(symlinks_dir) else {
        return Vec::new();
    };

    let mut stale = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        let file_type = metadata.file_type();
        if !file_type.is_symlink() && !metadata.is_file() {
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

fn find_orphan_install_entries(
    install_roots: &[&Path],
    tracked_install_paths: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut orphans = Vec::new();

    for root in install_roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !tracked_install_paths.contains(&path) {
                orphans.push(path);
            }
        }
    }

    orphans.sort();
    orphans.dedup();
    orphans
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

pub fn run(names: Vec<String>, verbose: bool, fix: bool) -> Result<()> {
    println!("{}", style("Running upstream doctor...").cyan());

    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let mut report = DoctorReport::new(verbose);

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
            report.hint(
                "Run `upstream hooks init` to create missing upstream directories and metadata files.",
            );
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
        report.hint("Run `upstream hooks init` to generate the default config file.");
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
        report.hint("Run `upstream hooks init` to create package metadata storage.");
    }

    check_paths_file(&paths, &mut report);
    if fix {
        fix_paths_file(&paths, &mut report);
    }

    let all_packages = package_storage.get_all_packages().to_vec();
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

    let tracked_install_paths: HashSet<PathBuf> = all_packages
        .iter()
        .filter_map(|package| package.install_path.clone())
        .collect();
    let orphan_install_entries = find_orphan_install_entries(
        &[
            paths.install.appimages_dir.as_path(),
            paths.install.binaries_dir.as_path(),
            paths.install.archives_dir.as_path(),
        ],
        &tracked_install_paths,
    );
    if orphan_install_entries.is_empty() {
        report.line(Level::Ok, "No untracked install artifacts detected");
    } else {
        let orphan_list = orphan_install_entries
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        report.line(
            Level::Warn,
            format!(
                "Detected {} untracked install artifact(s): {}",
                orphan_install_entries.len(),
                orphan_list
            ),
        );
        report.hint(
            "Delete untracked install artifacts manually, or recreate metadata and remove through upstream."
        );
    }

    let mut selected = Vec::new();
    if names.is_empty() {
        selected.extend(all_packages.iter().cloned());
        report.line(
            Level::Ok,
            format!("Loaded {} package(s) for checks", selected.len()),
        );
    } else {
        for name in &names {
            match package_storage.get_package_by_name(name) {
                Some(pkg) => selected.push(pkg.clone()),
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

    let mut changed_packages = false;
    let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);

    for package in &selected {
        let package_name = package.name.clone();
        let package_label = format!("package '{}'", package.name);
        let mut resolved_exec_path = package.exec_path.clone();

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

        match &resolved_exec_path {
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
                    if fix {
                        if let Err(err) = permission_handler::make_executable(path) {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} failed to set executable bit during fix: {}",
                                    package_label, err
                                ),
                            );
                        } else {
                            report.line(
                                Level::Ok,
                                format!("{} executable bit repaired", package_label),
                            );
                        }
                    }
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
                if fix && let Some(install_path) = &package.install_path {
                    let rediscovered = if install_path.is_file() {
                        Some(install_path.clone())
                    } else {
                        permission_handler::find_executable(install_path, &package.name)
                    };
                    if let Some(path) = rediscovered {
                        resolved_exec_path = Some(path.clone());
                        report.line(
                            Level::Ok,
                            format!(
                                "{} rediscovered executable path: {}",
                                package_label,
                                path.display()
                            ),
                        );
                    } else {
                        report.line(
                            Level::Warn,
                            format!("{} could not rediscover executable path", package_label),
                        );
                    }
                }
            }
        }

        if resolved_exec_path.is_some() {
            let link_path = expected_link_path(&paths.integration.symlinks_dir, &package.name);
            #[cfg(unix)]
            {
                let Some(exec_path) = &resolved_exec_path else {
                    unreachable!("checked above");
                };
                match inspect_unix_link(&link_path, exec_path) {
                    LinkStatus::Target {
                        raw_target,
                        resolved_target,
                        exists,
                        matches_expected,
                    } => {
                        if !exists {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} symlink target is missing ({} -> {}, resolved: {})",
                                    package_label,
                                    link_path.display(),
                                    raw_target.display(),
                                    resolved_target.display()
                                ),
                            );
                            report.hint(format!(
                                "Try `upstream upgrade {} --force` to recreate broken symlinks.",
                                package.name
                            ));
                        } else if matches_expected {
                            report.line(
                                Level::Ok,
                                format!("{} symlink points to executable", package_label),
                            );
                        } else {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} symlink target differs ({} -> {}, expected {})",
                                    package_label,
                                    link_path.display(),
                                    raw_target.display(),
                                    exec_path.display()
                                ),
                            );
                        }
                    }
                    LinkStatus::Missing => {
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
                        if fix {
                            if let Err(err) = symlink_manager.add_link(exec_path, &package.name) {
                                report.line(
                                    Level::Warn,
                                    format!(
                                        "{} failed to recreate symlink: {}",
                                        package_label, err
                                    ),
                                );
                            } else {
                                report.line(
                                    Level::Ok,
                                    format!("{} recreated missing symlink", package_label),
                                );
                            }
                        }
                    }
                    LinkStatus::NotSymlink => {
                        report.line(
                            Level::Warn,
                            format!(
                                "{} link path exists but is not a symlink ({})",
                                package_label,
                                link_path.display()
                            ),
                        );
                        report.hint(format!(
                            "Remove '{}' and run `upstream upgrade {} --force`.",
                            link_path.display(),
                            package.name
                        ));
                        if fix {
                            if let Err(err) = symlink_manager.add_link(exec_path, &package.name) {
                                report.line(
                                    Level::Warn,
                                    format!(
                                        "{} failed to replace non-symlink link path: {}",
                                        package_label, err
                                    ),
                                );
                            } else {
                                report.line(
                                    Level::Ok,
                                    format!("{} repaired link path", package_label),
                                );
                            }
                        }
                    }
                    LinkStatus::Unreadable(e) => report.line(
                        Level::Warn,
                        format!("{} symlink unreadable: {}", package_label, e),
                    ),
                }
            }
            #[cfg(not(unix))]
            {
                if link_path.exists() {
                    report.line(Level::Ok, format!("{} link entry exists", package_label));
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
                    if fix && let Some(exec_path) = &resolved_exec_path {
                        if let Err(err) = symlink_manager.add_link(exec_path, &package.name) {
                            report.line(
                                Level::Warn,
                                format!("{} failed to recreate link entry: {}", package_label, err),
                            );
                        } else {
                            report.line(
                                Level::Ok,
                                format!("{} recreated missing link", package_label),
                            );
                        }
                    }
                }
            }
        }

        if fix
            && resolved_exec_path != package.exec_path
            && let Some(mut_pkg) = package_storage.get_mut_package_by_name(&package_name)
        {
            mut_pkg.exec_path = resolved_exec_path;
            changed_packages = true;
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

    if fix && changed_packages {
        package_storage.save_packages()?;
    }

    println!();
    report.print_summary();

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
mod tests {
    use super::{
        DoctorReport, expected_link_path, find_orphan_install_entries, find_stale_symlink_names,
    };
    #[cfg(unix)]
    use super::{LinkStatus, inspect_unix_link};
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-doctor-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn expected_link_path_uses_platform_naming() {
        let base = Path::new("/tmp/upstream-doctor");
        let link = expected_link_path(base, "tool");

        #[cfg(windows)]
        assert_eq!(link.file_name().and_then(|n| n.to_str()), Some("tool.exe"));

        #[cfg(not(windows))]
        assert_eq!(link.file_name().and_then(|n| n.to_str()), Some("tool"));
    }

    #[test]
    fn find_stale_symlink_names_reports_orphans() {
        let root = temp_root("stale");
        fs::create_dir_all(&root).expect("create root");

        let installed = expected_link_path(&root, "installed");
        let orphan = expected_link_path(&root, "orphan");
        fs::write(&installed, b"x").expect("create installed link file");
        fs::write(&orphan, b"x").expect("create orphan link file");

        let installed_names = HashSet::from(["installed".to_string()]);
        let stale = find_stale_symlink_names(&root, &installed_names);
        assert_eq!(stale, vec!["orphan".to_string()]);

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn find_orphan_install_entries_reports_untracked_paths() {
        let root = temp_root("orphan-install");
        let appimages = root.join("appimages");
        let binaries = root.join("binaries");
        let archives = root.join("archives");
        fs::create_dir_all(&appimages).expect("create appimages root");
        fs::create_dir_all(&binaries).expect("create binaries root");
        fs::create_dir_all(&archives).expect("create archives root");

        let tracked = binaries.join("tracked-bin");
        let orphan_file = appimages.join("orphan.AppImage");
        let orphan_dir = archives.join("orphan-dir");
        fs::write(&tracked, b"x").expect("create tracked file");
        fs::write(&orphan_file, b"x").expect("create orphan file");
        fs::create_dir_all(&orphan_dir).expect("create orphan dir");

        let tracked_paths = HashSet::from([tracked]);
        let orphans = find_orphan_install_entries(
            &[appimages.as_path(), binaries.as_path(), archives.as_path()],
            &tracked_paths,
        );

        assert_eq!(orphans.len(), 2);
        assert!(orphans.contains(&orphan_dir));
        assert!(orphans.contains(&orphan_file));

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn find_stale_symlink_names_includes_dangling_symlinks() {
        let root = temp_root("stale-dangling");
        fs::create_dir_all(&root).expect("create root");

        let dangling = expected_link_path(&root, "dangling");
        let missing_target = root.join("does-not-exist");
        std::os::unix::fs::symlink(&missing_target, &dangling).expect("create dangling symlink");

        let stale = find_stale_symlink_names(&root, &HashSet::new());
        assert_eq!(stale, vec!["dangling".to_string()]);

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn inspect_unix_link_reports_missing_target() {
        let root = temp_root("inspect-dangling");
        fs::create_dir_all(&root).expect("create root");

        let link = expected_link_path(&root, "tool");
        let expected_exec = root.join("expected-bin");
        fs::write(&expected_exec, b"x").expect("create expected exec");
        let missing_target = root.join("missing-bin");
        std::os::unix::fs::symlink(&missing_target, &link).expect("create dangling symlink");

        let status = inspect_unix_link(&link, &expected_exec);
        match status {
            LinkStatus::Target {
                raw_target,
                resolved_target,
                exists,
                matches_expected,
            } => {
                assert_eq!(raw_target, missing_target);
                assert_eq!(resolved_target, missing_target);
                assert!(!exists);
                assert!(!matches_expected);
            }
            _ => panic!("expected dangling target status"),
        }

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn doctor_report_hint_deduplicates_entries() {
        let mut report = DoctorReport::new(false);
        report.hint("Run upstream hooks init");
        report.hint("Run upstream hooks init");
        report.hint("Reinstall package");

        assert_eq!(report.hints.len(), 2);
        assert!(
            report
                .hints
                .contains(&"Run upstream hooks init".to_string())
        );
        assert!(report.hints.contains(&"Reinstall package".to_string()));
    }

    #[test]
    fn doctor_report_tracks_counts_and_findings() {
        let mut report = DoctorReport::new(false);
        report.line(super::Level::Ok, "ok");
        report.line(super::Level::Warn, "warn one");
        report.line(super::Level::Warn, "warn two");
        report.line(super::Level::Fail, "fail one");

        assert_eq!(report.ok, 1);
        assert_eq!(report.warn, 2);
        assert_eq!(report.fail, 1);
        assert_eq!(report.total_checks(), 4);
        assert_eq!(
            report.warnings,
            vec!["warn one".to_string(), "warn two".to_string()]
        );
        assert_eq!(report.failures, vec!["fail one".to_string()]);
    }
}
