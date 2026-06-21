use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::routines::migrate::MigrationReport;
use crate::utils::filesystem::safe_move;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug, Clone)]
pub(in crate::routines::migrate) struct PathRewrite {
    pub old: PathBuf,
    pub new: PathBuf,
}

pub(in crate::routines::migrate) fn create_required_dirs(
    paths: &UpstreamPaths,
    report: &mut MigrationReport,
) -> Result<()> {
    for dir in [
        paths.dirs.config_dir.as_path(),
        paths.dirs.data_dir.as_path(),
        paths.dirs.packages_dir.as_path(),
        paths.dirs.cache_dir.as_path(),
        paths.dirs.metadata_dir.as_path(),
        paths.install.appimages_dir.as_path(),
        paths.install.binaries_dir.as_path(),
        paths.install.archives_dir.as_path(),
        paths.install.rollback_dir.as_path(),
        paths.install.tmp_dir.as_path(),
        paths.integration.icons_dir.as_path(),
        paths.integration.symlinks_dir.as_path(),
    ] {
        if !dir.exists() {
            report.created_dirs += 1;
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory '{}'", dir.display()))?;
    }
    Ok(())
}

pub(in crate::routines::migrate) fn package_path_rewrites(
    paths: &UpstreamPaths,
) -> Vec<PathRewrite> {
    vec![
        PathRewrite {
            old: paths.dirs.data_dir.join("appimages"),
            new: paths.install.appimages_dir.clone(),
        },
        PathRewrite {
            old: paths.dirs.data_dir.join("binaries"),
            new: paths.install.binaries_dir.clone(),
        },
        PathRewrite {
            old: paths.dirs.data_dir.join("archives"),
            new: paths.install.archives_dir.clone(),
        },
    ]
}

pub(in crate::routines::migrate) fn legacy_package_dirs_exist(rewrites: &[PathRewrite]) -> bool {
    rewrites.iter().any(|rewrite| rewrite.old.exists())
}

pub(in crate::routines::migrate) fn move_legacy_package_dirs(
    rewrites: &[PathRewrite],
    report: &mut MigrationReport,
) -> Result<()> {
    for rewrite in rewrites {
        if !rewrite.old.exists() {
            continue;
        }
        move_into_layout(&rewrite.old, &rewrite.new, report).with_context(|| {
            format!(
                "Failed to migrate '{}' to '{}'",
                rewrite.old.display(),
                rewrite.new.display()
            )
        })?;
    }
    Ok(())
}

fn move_into_layout(src: &Path, dst: &Path, report: &mut MigrationReport) -> Result<()> {
    if paths_are_same(src, dst)? {
        return Ok(());
    }

    if !dst.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
        }
        safe_move::move_file_or_dir(src, dst)?;
        report.moved_entries += 1;
        return Ok(());
    }

    merge_directory_contents(src, dst, report)?;
    remove_dir_if_empty(src)?;
    Ok(())
}

fn merge_directory_contents(src: &Path, dst: &Path, report: &mut MigrationReport) -> Result<()> {
    for entry in fs::read_dir(src)
        .with_context(|| format!("Failed to read directory '{}'", src.display()))?
    {
        let entry =
            entry.with_context(|| format!("Failed to read entry in '{}'", src.display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to inspect '{}'", from.display()))?;

        if to.exists() {
            if file_type.is_dir() && to.is_dir() {
                merge_directory_contents(&from, &to, report)?;
                remove_dir_if_empty(&from)?;
                continue;
            }
            return Err(anyhow!(
                "Refusing to overwrite existing migrated path '{}'",
                to.display()
            ));
        }

        safe_move::move_file_or_dir(&from, &to)?;
        report.moved_entries += 1;
    }
    Ok(())
}

fn remove_dir_if_empty(path: &Path) -> Result<()> {
    if path.exists()
        && path
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    {
        fs::remove_dir(path)
            .with_context(|| format!("Failed to remove empty directory '{}'", path.display()))?;
    }
    Ok(())
}

fn paths_are_same(a: &Path, b: &Path) -> io::Result<bool> {
    if !a.exists() || !b.exists() {
        return Ok(false);
    }
    Ok(fs::canonicalize(a)? == fs::canonicalize(b)?)
}
