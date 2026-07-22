use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use indicatif::HumanBytes;
use serde::Serialize;
use walkdir::WalkDir;

use crate::{
    application::cli::arguments::CacheKind,
    output::{self, Status},
    utils::static_paths::UpstreamPaths,
};

#[derive(Debug, Clone, Serialize)]
struct CacheEntry {
    kind: &'static str,
    path: PathBuf,
    exists: bool,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct CacheReport {
    caches: Vec<CacheEntry>,
    total_bytes: u64,
}

pub fn run_list(json: bool, paths: &UpstreamPaths) -> Result<()> {
    let report = inspect(paths)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("{}", output::title("Cache"));
    for entry in &report.caches {
        let detail = if entry.exists {
            format!("{}  {}", HumanBytes(entry.bytes), entry.path.display())
        } else {
            format!("empty  {}", entry.path.display())
        };
        output::status_line(Status::Plan, entry.kind, detail);
    }
    println!("Total: {}", HumanBytes(report.total_bytes));
    Ok(())
}

pub fn run_clean(categories: Vec<CacheKind>, dry_run: bool, paths: &UpstreamPaths) -> Result<()> {
    let selected = selected_kinds(&categories)?;
    let report = inspect_selected(paths, &selected)?;

    println!(
        "{}",
        output::title(if dry_run {
            "Cache clean (dry run)"
        } else {
            "Cache clean"
        })
    );
    for entry in &report.caches {
        output::status_line(
            if entry.exists {
                Status::Plan
            } else {
                Status::Skip
            },
            entry.kind,
            format!("{}  {}", HumanBytes(entry.bytes), entry.path.display()),
        );
    }
    println!("Reclaimable: {}", HumanBytes(report.total_bytes));

    if dry_run || report.caches.iter().all(|entry| !entry.exists) {
        return Ok(());
    }

    output::confirm_or_cancel(
        format!("Remove {} selected cache category(s)?", report.caches.len()),
        false,
    )?;

    for entry in &report.caches {
        remove_path_without_following(&entry.path)
            .with_context(|| format!("Failed to clean {} cache", entry.kind))?;
        output::status_line(Status::Ok, entry.kind, "cleaned");
    }
    println!(
        "{}",
        output::success(format!("Reclaimed {}.", HumanBytes(report.total_bytes)))
    );
    Ok(())
}

fn selected_kinds(categories: &[CacheKind]) -> Result<Vec<CacheKind>> {
    if categories.is_empty() || categories == [CacheKind::All] {
        return Ok(vec![
            CacheKind::Build,
            CacheKind::Source,
            CacheKind::Docs,
            CacheKind::Registry,
        ]);
    }
    if categories.contains(&CacheKind::All) {
        bail!("Cache category 'all' cannot be combined with other categories");
    }
    let mut selected = Vec::new();
    for kind in categories {
        if !selected.contains(kind) {
            selected.push(*kind);
        }
    }
    Ok(selected)
}

fn inspect(paths: &UpstreamPaths) -> Result<CacheReport> {
    inspect_selected(
        paths,
        &[
            CacheKind::Build,
            CacheKind::Source,
            CacheKind::Docs,
            CacheKind::Registry,
        ],
    )
}

fn inspect_selected(paths: &UpstreamPaths, selected: &[CacheKind]) -> Result<CacheReport> {
    let mut caches = Vec::new();
    for kind in selected {
        let (label, path) = cache_path(paths, *kind);
        let metadata = fs::symlink_metadata(&path).ok();
        let exists = metadata.is_some();
        let bytes = path_size_without_following(&path)?;
        caches.push(CacheEntry {
            kind: label,
            path,
            exists,
            bytes,
        });
    }
    let total_bytes = caches.iter().map(|entry| entry.bytes).sum();
    Ok(CacheReport {
        caches,
        total_bytes,
    })
}

fn cache_path(paths: &UpstreamPaths, kind: CacheKind) -> (&'static str, PathBuf) {
    let child = match kind {
        CacheKind::Build => "build",
        CacheKind::Source => "source",
        CacheKind::Docs => "docs",
        CacheKind::Registry => "registry",
        CacheKind::All => unreachable!("all is expanded before resolving paths"),
    };
    (child, paths.dirs.cache_dir.join(child))
}

fn path_size_without_following(path: &Path) -> Result<u64> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(0);
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        return Ok(metadata.len());
    }

    let mut total = 0_u64;
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.with_context(|| format!("Failed to scan cache '{}'", path.display()))?;
        if entry.path() == path {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .with_context(|| format!("Failed to inspect '{}'", entry.path().display()))?;
        if metadata.is_file() || metadata.file_type().is_symlink() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn remove_path_without_following(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory '{}'", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("Failed to remove '{}'", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::{inspect, selected_kinds};
    use crate::{application::cli::arguments::CacheKind, utils::test_support};
    use std::fs;

    #[test]
    fn inspection_counts_known_categories_and_ignores_unknown_entries() {
        let root = test_support::temp_root("cache-command", "inspect");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.dirs.cache_dir.join("build/nested")).expect("build cache");
        fs::write(paths.dirs.cache_dir.join("build/nested/data"), b"1234").expect("build data");
        fs::create_dir_all(paths.dirs.cache_dir.join("unknown")).expect("unknown cache");
        fs::write(paths.dirs.cache_dir.join("unknown/data"), b"ignored").expect("unknown data");

        let report = inspect(&paths).expect("inspect cache");

        assert_eq!(report.caches.len(), 4);
        assert_eq!(report.total_bytes, 4);
        assert_eq!(report.caches[0].kind, "build");
        assert_eq!(report.caches[0].bytes, 4);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn category_selection_expands_all_and_rejects_mixed_all() {
        assert_eq!(
            selected_kinds(&[]).expect("default categories"),
            vec![
                CacheKind::Build,
                CacheKind::Source,
                CacheKind::Docs,
                CacheKind::Registry,
            ]
        );
        assert!(selected_kinds(&[CacheKind::All, CacheKind::Docs]).is_err());
        assert_eq!(
            selected_kinds(&[CacheKind::Docs, CacheKind::Docs]).expect("deduplicate"),
            vec![CacheKind::Docs]
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleaning_a_symlink_does_not_remove_its_target() {
        use super::remove_path_without_following;
        use std::os::unix::fs::symlink;

        let root = test_support::temp_root("cache-command", "symlink");
        let outside = root.join("outside");
        let cache_link = root.join("cache/build");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(outside.join("keep"), b"safe").expect("outside data");
        fs::create_dir_all(cache_link.parent().expect("cache parent")).expect("cache root");
        symlink(&outside, &cache_link).expect("cache symlink");

        remove_path_without_following(&cache_link).expect("remove link");

        assert!(!cache_link.exists());
        assert!(outside.join("keep").exists());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
