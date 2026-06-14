use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::utils::filesystem::atomic_ops::write_atomic;

const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManifestFile {
    version: u32,
    entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestEntry {
    Directory,
    File,
    Symlink { target: String },
}

impl Default for ManifestFile {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

/// Sync a source tree into a destination while deleting only paths previously
/// owned by this manifest. Untracked destination paths are preserved.
pub fn sync_manifested_tree(source: &Path, destination: &Path, manifest_path: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(anyhow!(
            "Manifest sync source '{}' is not a directory",
            source.display()
        ));
    }

    let previous = load_manifest(manifest_path)?;
    let current = collect_manifest(source)?;

    fs::create_dir_all(destination).with_context(|| {
        format!(
            "Failed to create manifest sync destination '{}'",
            destination.display()
        )
    })?;

    remove_stale_entries(destination, &previous, &current)?;
    copy_current_entries(source, destination, &current)?;
    prune_empty_stale_dirs(destination, &previous, &current)?;
    save_manifest(manifest_path, &current)
}

fn load_manifest(path: &Path) -> Result<ManifestFile> {
    if !path.exists() {
        return Ok(ManifestFile::default());
    }

    let json = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest '{}'", path.display()))?;
    if json.trim().is_empty() {
        return Ok(ManifestFile::default());
    }

    let manifest: ManifestFile = serde_json::from_str(&json)
        .with_context(|| format!("Failed to parse manifest '{}'", path.display()))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(anyhow!(
            "Unsupported manifest version {} in '{}'. Expected {}.",
            manifest.version,
            path.display(),
            MANIFEST_VERSION
        ));
    }

    Ok(manifest)
}

fn save_manifest(path: &Path, manifest: &ManifestFile) -> Result<()> {
    let json = serde_json::to_string_pretty(manifest).context("Failed to serialize manifest")?;
    write_atomic(path, json.as_bytes())
        .with_context(|| format!("Failed to write manifest '{}'", path.display()))
}

fn collect_manifest(source: &Path) -> Result<ManifestFile> {
    let mut manifest = ManifestFile::default();
    collect_entries(source, source, &mut manifest.entries)?;
    Ok(manifest)
}

fn collect_entries(
    source_root: &Path,
    current: &Path,
    entries: &mut BTreeMap<String, ManifestEntry>,
) -> Result<()> {
    let mut children = fs::read_dir(current)
        .with_context(|| format!("Failed to read source directory '{}'", current.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to read source directory '{}'", current.display()))?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        let path = child.path();
        let relative = normalized_relative(source_root, &path)?;
        let file_type = child
            .file_type()
            .with_context(|| format!("Failed to inspect '{}'", path.display()))?;

        if file_type.is_symlink() {
            let target = fs::read_link(&path)
                .with_context(|| format!("Failed to read symlink '{}'", path.display()))?;
            entries.insert(
                relative,
                ManifestEntry::Symlink {
                    target: target.to_string_lossy().to_string(),
                },
            );
        } else if file_type.is_dir() {
            entries.insert(relative, ManifestEntry::Directory);
            collect_entries(source_root, &path, entries)?;
        } else if file_type.is_file() {
            entries.insert(relative, ManifestEntry::File);
        }
    }

    Ok(())
}

fn remove_stale_entries(
    destination: &Path,
    previous: &ManifestFile,
    current: &ManifestFile,
) -> Result<()> {
    for (relative, entry) in previous.entries.iter().rev() {
        if current.entries.contains_key(relative) {
            continue;
        }
        let path = destination.join(relative);
        remove_manifest_entry(&path, entry)?;
    }
    Ok(())
}

fn remove_manifest_entry(path: &Path, entry: &ManifestEntry) -> Result<()> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    match entry {
        ManifestEntry::Directory => {
            if path.is_dir() {
                let _ = fs::remove_dir(path);
            }
        }
        ManifestEntry::File | ManifestEntry::Symlink { .. } => {
            if path.is_dir() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("Failed to remove directory '{}'", path.display()))?;
            } else {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove file '{}'", path.display()))?;
            }
        }
    }

    Ok(())
}

fn copy_current_entries(source: &Path, destination: &Path, manifest: &ManifestFile) -> Result<()> {
    for (relative, entry) in &manifest.entries {
        let source_path = source.join(relative);
        let destination_path = destination.join(relative);
        match entry {
            ManifestEntry::Directory => {
                replace_non_directory(&destination_path)?;
                fs::create_dir_all(&destination_path).with_context(|| {
                    format!(
                        "Failed to create directory '{}'",
                        destination_path.display()
                    )
                })?;
            }
            ManifestEntry::File => copy_file_if_changed(&source_path, &destination_path)?,
            ManifestEntry::Symlink { target } => sync_symlink(&destination_path, target)?,
        }
    }
    Ok(())
}

fn replace_non_directory(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() {
        return Ok(());
    }
    fs::remove_file(path).with_context(|| format!("Failed to remove file '{}'", path.display()))
}

fn copy_file_if_changed(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    replace_non_file(destination)?;
    if destination.is_file() && files_equal(source, destination)? {
        return Ok(());
    }

    fs::copy(source, destination).with_context(|| {
        format!(
            "Failed to copy '{}' to '{}'",
            source.display(),
            destination.display()
        )
    })?;
    let permissions = fs::metadata(source)
        .with_context(|| format!("Failed to inspect '{}'", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("Failed to set permissions on '{}'", destination.display()))
}

fn replace_non_file(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_file() && !metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory '{}'", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("Failed to remove file '{}'", path.display()))
    }
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left_meta =
        fs::metadata(left).with_context(|| format!("Failed to inspect '{}'", left.display()))?;
    let right_meta =
        fs::metadata(right).with_context(|| format!("Failed to inspect '{}'", right.display()))?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }
    Ok(
        fs::read(left).with_context(|| format!("Failed to read '{}'", left.display()))?
            == fs::read(right).with_context(|| format!("Failed to read '{}'", right.display()))?,
    )
}

#[cfg(unix)]
fn sync_symlink(destination: &Path, target: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
    }

    if let Ok(existing) = fs::read_link(destination)
        && existing == Path::new(target)
    {
        return Ok(());
    }

    remove_existing_path(destination)?;
    symlink(target, destination)
        .with_context(|| format!("Failed to create symlink '{}'", destination.display()))
}

#[cfg(not(unix))]
fn sync_symlink(destination: &Path, target: &str) -> Result<()> {
    let target_path = std::path::PathBuf::from(target);
    if target_path.is_file() {
        copy_file_if_changed(&target_path, destination)
    } else {
        Err(anyhow!(
            "Cannot sync symlink '{}' on this platform",
            destination.display()
        ))
    }
}

fn remove_existing_path(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory '{}'", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("Failed to remove file '{}'", path.display()))
    }
}

fn prune_empty_stale_dirs(
    destination: &Path,
    previous: &ManifestFile,
    current: &ManifestFile,
) -> Result<()> {
    let previous_dirs = previous
        .entries
        .iter()
        .filter_map(|(path, entry)| matches!(entry, ManifestEntry::Directory).then_some(path))
        .collect::<BTreeSet<_>>();
    let current_dirs = current
        .entries
        .iter()
        .filter_map(|(path, entry)| matches!(entry, ManifestEntry::Directory).then_some(path))
        .collect::<BTreeSet<_>>();

    let mut stale_dirs = previous_dirs
        .difference(&current_dirs)
        .copied()
        .collect::<Vec<_>>();
    stale_dirs.sort_by_key(|relative| std::cmp::Reverse(relative.matches('/').count()));

    for relative in stale_dirs {
        let path = destination.join(relative);
        if path.is_dir() {
            let _ = fs::remove_dir(&path);
        }
    }
    Ok(())
}

fn normalized_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("Failed to relativize '{}'", path.display()))?;
    if relative.as_os_str().is_empty() {
        return Err(anyhow!("Manifest entry cannot be empty"));
    }

    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            _ => {
                return Err(anyhow!(
                    "Manifest entry '{}' contains unsupported path component",
                    relative.display()
                ));
            }
        }
    }

    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::sync_manifested_tree;
    use std::{fs, path::PathBuf, time::SystemTime};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-manifest-sync-test-{name}-{nanos}"))
    }

    #[test]
    fn sync_updates_owned_files_and_preserves_unowned_build_output() {
        let root = temp_root("preserve");
        let source = root.join("source");
        let destination = root.join("destination");
        let manifest = root.join("manifest.json");

        fs::create_dir_all(source.join("src")).expect("create source");
        fs::write(source.join("src/main.rs"), "fn main() {}\n").expect("write source");
        sync_manifested_tree(&source, &destination, &manifest).expect("first sync");

        fs::create_dir_all(destination.join("target")).expect("create build output");
        fs::write(destination.join("target/cache.o"), "object").expect("write build output");

        fs::write(
            source.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .expect("update source");
        fs::write(source.join("src/lib.rs"), "pub fn lib() {}\n").expect("new source");
        sync_manifested_tree(&source, &destination, &manifest).expect("second sync");

        assert_eq!(
            fs::read_to_string(destination.join("src/main.rs")).expect("read main"),
            "fn main() { println!(\"hi\"); }\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("src/lib.rs")).expect("read lib"),
            "pub fn lib() {}\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("target/cache.o")).expect("read build output"),
            "object"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_removes_owned_files_missing_from_new_source() {
        let root = temp_root("remove-owned");
        let source = root.join("source");
        let destination = root.join("destination");
        let manifest = root.join("manifest.json");

        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("old.rs"), "old").expect("write old");
        sync_manifested_tree(&source, &destination, &manifest).expect("first sync");

        fs::remove_file(source.join("old.rs")).expect("remove source");
        sync_manifested_tree(&source, &destination, &manifest).expect("second sync");

        assert!(!destination.join("old.rs").exists());
        let _ = fs::remove_dir_all(root);
    }
}
