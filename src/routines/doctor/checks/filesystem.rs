use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    models::upstream::{AppConfig, Package},
    storage::config_storage::ConfigStorage,
    utils::static_paths::UpstreamPaths,
};

use super::super::{DoctorReport, Level};
use super::legacy::{MIGRATE_DIR_HINT, looks_like_legacy_layout};

const HOOKS_INIT_DIR_HINT: &str =
    "Run `upstream hooks init` to create missing upstream directories and metadata files.";

fn required_directory_checks(paths: &UpstreamPaths) -> Vec<(&'static str, &Path)> {
    vec![
        ("data directory", paths.dirs.data_dir.as_path()),
        ("packages directory", paths.dirs.packages_dir.as_path()),
        ("cache directory", paths.dirs.cache_dir.as_path()),
        ("metadata directory", paths.dirs.metadata_dir.as_path()),
        (
            "symlinks directory",
            paths.integration.symlinks_dir.as_path(),
        ),
        ("icons directory", paths.integration.icons_dir.as_path()),
        ("appimages directory", paths.install.appimages_dir.as_path()),
        ("binaries directory", paths.install.binaries_dir.as_path()),
        ("archives directory", paths.install.archives_dir.as_path()),
        ("tmp directory", paths.install.tmp_dir.as_path()),
    ]
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

pub(in crate::routines::doctor) fn check_local_layout(
    paths: &UpstreamPaths,
    report: &mut DoctorReport,
) {
    let legacy_layout_detected = looks_like_legacy_layout(paths);
    if legacy_layout_detected {
        report.line(Level::Warn, "legacy upstream data layout detected");
        report.hint(MIGRATE_DIR_HINT);
    }

    for (label, path) in required_directory_checks(paths) {
        if path.exists() {
            report.line(Level::Ok, format!("{} exists", label));
        } else {
            report.line(
                Level::Fail,
                format!("{} missing: {}", label, path.display()),
            );
            report.hint(if legacy_layout_detected {
                MIGRATE_DIR_HINT
            } else {
                HOOKS_INIT_DIR_HINT
            });
        }
    }
}

pub(in crate::routines::doctor) fn load_app_config(
    paths: &UpstreamPaths,
    report: &mut DoctorReport,
) -> Option<AppConfig> {
    if paths.config.config_file.exists() {
        match ConfigStorage::new(&paths.config.config_file) {
            Ok(storage) => {
                report.line(Level::Ok, "Config file exists");
                Some(storage.get_config().clone())
            }
            Err(err) => {
                report.line(Level::Fail, format!("Config file is invalid: {err}"));
                report.hint(MIGRATE_DIR_HINT);
                None
            }
        }
    } else {
        report.line(
            Level::Warn,
            format!(
                "Config file missing: {}",
                paths.config.config_file.display()
            ),
        );
        report.hint("Run `upstream hooks init` to generate the default config file.");
        None
    }
}

pub(in crate::routines::doctor) fn check_package_metadata_file(
    paths: &UpstreamPaths,
    report: &mut DoctorReport,
) {
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
}

pub(in crate::routines::doctor) fn check_untracked_package_artifacts(
    paths: &UpstreamPaths,
    all_packages: &[Package],
    report: &mut DoctorReport,
) {
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{HOOKS_INIT_DIR_HINT, find_orphan_install_entries, find_stale_symlink_names};
    use crate::routines::doctor::checks::legacy::MIGRATE_DIR_HINT;
    use crate::routines::doctor::checks::packages::expected_link_path;

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

    #[test]
    fn directory_hints_are_easy_to_distinguish() {
        assert!(MIGRATE_DIR_HINT.contains("upstream doctor --migrate"));
        assert!(HOOKS_INIT_DIR_HINT.contains("upstream hooks init"));
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
}
