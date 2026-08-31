use anyhow::Result;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    models::upstream::{InstallType, Package},
    services::{artifact::permission_handler, integration::SymlinkManager},
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

use super::super::{DoctorReport, Level};

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
    package_database: Option<&PackageDatabase>,
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
            match package_database.and_then(|database| database.get_package(name).ok().flatten()) {
                Some(package) => selected.push(package),
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

pub(in crate::routines::doctor) async fn check_version_tag_templates(
    package_database: &mut PackageDatabase,
    selected: &[Package],
    fix: bool,
    report: &mut DoctorReport,
) -> Result<()> {
    for package in selected {
        if package.install_type != InstallType::Release || package.version.is_unknown() {
            continue;
        }

        if package.release_tag.is_some() {
            report.line(
                Level::Ok,
                format!("package '{}' exact release tag exists", package.id),
            );
            continue;
        }

        if package.version_tag_template.is_some() {
            report.line(
                Level::Ok,
                format!("package '{}' version tag template exists", package.id),
            );
            continue;
        }

        if fix {
            package_database.update_package(&package.id, |package| {
                package.version_tag_template = Some("v{}".to_string());
                Ok(true)
            })?;
            report.line(
                Level::Ok,
                format!("package '{}' repaired version tag template", package.id),
            );
        } else {
            report.line(
                Level::Warn,
                format!("package '{}' is missing version tag template", package.id),
            );
            report.hint("Run `upstream doctor --fix` to repair version tag templates.");
        }
    }

    Ok(())
}

pub(in crate::routines::doctor) fn check_installed_packages(
    paths: &UpstreamPaths,
    _package_database: &mut PackageDatabase,
    selected: &[Package],
    fix: bool,
    report: &mut DoctorReport,
) -> Result<()> {
    let symlink_manager = SymlinkManager::new(&paths.state.symlinks_dir);

    for package in selected {
        let package_label = format!("package '{}'", package.id);

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
                    package.id, package.id
                ));
            }
        }

        if package.executables.is_empty() {
            report.line(
                Level::Warn,
                format!("{} has no executable aliases recorded", package_label),
            );
            report.hint(format!(
                "Try `upstream reinstall {}` to rebuild executable aliases.",
                package.id
            ));
        }

        for executable in &package.executables {
            let executable_label = format!("{} executable '{}'", package_label, executable.name);
            let path = &executable.path;
            if path.exists() {
                if is_executable(path) {
                    report.line(Level::Ok, format!("{} path is valid", executable_label));
                } else {
                    report.line(
                        Level::Warn,
                        format!("{} is not marked executable", executable_label),
                    );
                    if fix {
                        if let Err(err) = permission_handler::make_executable(path) {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} failed to set executable bit during fix: {}",
                                    executable_label, err
                                ),
                            );
                        } else {
                            report.line(
                                Level::Ok,
                                format!("{} executable bit repaired", executable_label),
                            );
                        }
                    }
                }
            } else {
                report.line(
                    Level::Fail,
                    format!("{} path missing: {}", executable_label, path.display()),
                );
            }
            let link_path = expected_link_path(&paths.state.symlinks_dir, &executable.name);
            #[cfg(unix)]
            {
                match inspect_unix_link(&link_path, path) {
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
                                    executable_label,
                                    link_path.display(),
                                    raw_target.display(),
                                    resolved_target.display()
                                ),
                            );
                            report.hint(format!(
                                "Try `upstream reinstall {}` to recreate broken symlinks.",
                                executable.name
                            ));
                        } else if matches_expected {
                            report.line(
                                Level::Ok,
                                format!("{} symlink points to executable", executable_label),
                            );
                        } else {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} symlink target differs ({} -> {}, expected {})",
                                    executable_label,
                                    link_path.display(),
                                    raw_target.display(),
                                    path.display()
                                ),
                            );
                        }
                    }
                    LinkStatus::Missing => {
                        report.line(
                            Level::Warn,
                            format!(
                                "{} link missing in symlinks dir ({})",
                                executable_label,
                                link_path.display()
                            ),
                        );
                        report.hint(format!(
                            "Try `upstream reinstall {}` to recreate missing links.",
                            executable.name
                        ));
                        if fix {
                            if let Err(err) = symlink_manager.add_link(path, &executable.name) {
                                report.line(
                                    Level::Warn,
                                    format!(
                                        "{} failed to recreate symlink: {}",
                                        executable_label, err
                                    ),
                                );
                            } else {
                                report.line(
                                    Level::Ok,
                                    format!("{} recreated missing symlink", executable_label),
                                );
                            }
                        }
                    }
                    LinkStatus::NotSymlink => {
                        report.line(
                            Level::Warn,
                            format!(
                                "{} link path exists but is not a symlink ({})",
                                executable_label,
                                link_path.display()
                            ),
                        );
                        report.hint(format!(
                            "Remove '{}' and run `upstream reinstall {}`.",
                            link_path.display(),
                            executable.name
                        ));
                        if fix {
                            if let Err(err) = symlink_manager.add_link(path, &executable.name) {
                                report.line(
                                    Level::Warn,
                                    format!(
                                        "{} failed to replace non-symlink link path: {}",
                                        executable_label, err
                                    ),
                                );
                            } else {
                                report.line(
                                    Level::Ok,
                                    format!("{} repaired link path", executable_label),
                                );
                            }
                        }
                    }
                    LinkStatus::Unreadable(e) => report.line(
                        Level::Warn,
                        format!("{} symlink unreadable: {}", executable_label, e),
                    ),
                }
            }

            #[cfg(not(unix))]
            {
                if link_path.exists() {
                    report.line(Level::Ok, format!("{} link entry exists", executable_label));
                } else {
                    report.line(
                        Level::Warn,
                        format!(
                            "{} link missing in symlinks dir ({})",
                            executable_label,
                            link_path.display()
                        ),
                    );
                    report.hint(format!(
                        "Try `upstream reinstall {}` to recreate missing links.",
                        executable.name
                    ));
                    if fix {
                        if let Err(err) = symlink_manager.add_link(path, &executable.name) {
                            report.line(
                                Level::Warn,
                                format!(
                                    "{} failed to recreate link entry: {}",
                                    executable_label, err
                                ),
                            );
                        } else {
                            report.line(
                                Level::Ok,
                                format!("{} recreated missing link", executable_label),
                            );
                        }
                    }
                }
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
                    .join(format!("{}.desktop", package.id));

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
                        package.id
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

    #[cfg(unix)]
    use super::{LinkStatus, inspect_unix_link};
    use super::{check_version_tag_templates, expected_link_path};
    use crate::models::{
        common::{
            Version,
            enums::{Channel, Filetype, Provider},
        },
        upstream::Package,
    };
    use crate::routines::doctor::DoctorReport;
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;

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

    #[tokio::test]
    async fn fix_version_tag_templates_uses_v_template_without_network() {
        let root = test_support::temp_root("upstream-doctor-test", "version-tag-template");
        let paths = test_support::upstream_paths(&root);
        let mut db =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open db");

        let mut package = Package::with_defaults(
            "codex".to_string(),
            "openai/codex".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.version = Version::new(0, 142, 0, false);
        db.upsert_package(&package).expect("upsert package");
        let selected = vec![package];
        let mut report = DoctorReport::new();

        check_version_tag_templates(&mut db, &selected, true, &mut report)
            .await
            .expect("repair templates");

        let stored = db
            .get_package("codex")
            .expect("load package")
            .expect("package exists");

        assert_eq!(stored.version_tag_template.as_deref(), Some("v{}"));
        assert!(report.warnings.is_empty());
        assert!(report.hints.is_empty());

        std::fs::remove_dir_all(&root).expect("cleanup");
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
