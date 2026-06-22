use anyhow::Result;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    models::upstream::Package,
    services::{
        artifact::permission_handler,
        integration::{CompletionManager, SymlinkManager},
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

use super::super::{DoctorReport, Level};
use super::integration::check_completion_cache_drift;

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

pub(in crate::routines::doctor::checks) fn expected_link_path(
    base_dir: &Path,
    name: &str,
) -> PathBuf {
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

pub(in crate::routines::doctor) fn select_packages(
    names: &[String],
    all_packages: &[Package],
    report: &mut DoctorReport,
) -> Vec<Package> {
    let mut selected = Vec::new();
    if names.is_empty() {
        selected.extend(all_packages.iter().cloned());
        report.line(
            Level::Ok,
            format!("Loaded {} package(s) for checks", selected.len()),
        );
    } else {
        for name in names {
            match all_packages.iter().find(|package| package.name == *name) {
                Some(package) => selected.push(package.clone()),
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

    selected
}

pub(in crate::routines::doctor) fn check_installed_packages(
    paths: &UpstreamPaths,
    package_database: &mut PackageDatabase,
    selected: &[Package],
    completion_manager: &CompletionManager<'_>,
    fix: bool,
    report: &mut DoctorReport,
) -> Result<()> {
    let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);

    for package in selected {
        let package_name = package.name.clone();
        let package_label = format!("package '{}'", package.name);
        let mut resolved_exec_path = package.exec_path.clone();

        check_completion_cache_drift(completion_manager, &package.name, fix, report);

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
                    "Try `upstream reinstall {}` to rebuild executable metadata.",
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
                                "Try `upstream reinstall {}` to recreate broken symlinks.",
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
                            "Try `upstream reinstall {}` to recreate missing links.",
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
                            "Remove '{}' and run `upstream reinstall {}`.",
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
                        "Try `upstream reinstall {}` to recreate missing links.",
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

        if fix && resolved_exec_path != package.exec_path {
            package_database.update_package(&package_name, |package| {
                package.exec_path = resolved_exec_path.clone();
                Ok(true)
            })?;
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

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::io;
    use std::path::Path;
    #[cfg(unix)]
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::expected_link_path;
    #[cfg(unix)]
    use super::{LinkStatus, inspect_unix_link};

    #[cfg(unix)]
    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-doctor-test-{name}-{nanos}"))
    }

    #[cfg(unix)]
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
}
