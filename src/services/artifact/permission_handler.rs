#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use std::path::Path;
use std::{fs, path::PathBuf};

/// Sets executable permissions on a file for user, group, and others.
#[cfg(unix)]
pub fn make_executable(exec_path: &Path) -> Result<()> {
    if !exec_path.exists() {
        anyhow::bail!("Invalid executable path: {}", exec_path.to_string_lossy());
    }

    match fs::metadata(exec_path) {
        Ok(metadata) => {
            let mut permissions = metadata.permissions();
            let mode = permissions.mode();

            permissions.set_mode(mode | 0o111);

            fs::set_permissions(exec_path, permissions)
                .context("Failed to set executable permissions")?;
        }
        Err(e) => {
            return Err(e).context("Failed to read metadata");
        }
    }

    Ok(())
}

#[cfg(windows)]
pub fn make_executable(_exec_path: &Path) -> Result<()> {
    Ok(())
}

/// Find executable files in an extracted artifact. A non-empty \`bin\` directory
/// takes precedence over the artifact root; only one of those directories is
/// scanned.
pub fn find_executables(directory_path: &Path, preferred_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let bin_directory = directory_path.join("bin");
    let directory =
        if fs::read_dir(&bin_directory).is_ok_and(|mut entries| entries.next().is_some()) {
            bin_directory
        } else {
            directory_path.to_path_buf()
        };

    let Ok(entries) = fs::read_dir(directory) else {
        return paths;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };

        if kind.is_file() && !is_shared_library(&path) && is_executable_file(&path) {
            paths.push(path);
        }
    }

    paths.sort_by(|left, right| {
        let left_preferred = left
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(preferred_name));

        let right_preferred = right
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(preferred_name));

        right_preferred
            .cmp(&left_preferred)
            .then_with(|| left.cmp(right))
    });
    paths
}

fn is_shared_library(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "so" | "dll" | "dylib"
            )
        })
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "exe" | "cmd" | "bat"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::find_executables;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        std::env::temp_dir().join(format!("upstream-perm-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn executable_name(base: &str) -> String {
        #[cfg(windows)]
        {
            format!("{base}.exe")
        }

        #[cfg(not(windows))]
        {
            base.to_string()
        }
    }

    #[test]
    fn scans_only_non_empty_bin_directory() {
        let root = temp_root("direct");
        fs::create_dir_all(&root).expect("create root");
        let root_executable = root.join(executable_name("tool"));
        let bin_executable = root.join("bin").join(executable_name("helper"));
        let nested_executable = root.join("nested").join(executable_name("ignored"));
        fs::create_dir_all(bin_executable.parent().expect("bin parent")).expect("create bin");
        fs::create_dir_all(nested_executable.parent().expect("nested parent"))
            .expect("create nested");
        for path in [&root_executable, &bin_executable, &nested_executable] {
            fs::write(path, b"#!/bin/sh\n").expect("write executable");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                    .expect("make executable");
            }
        }

        let found = find_executables(&root, "tool");
        assert_eq!(found, vec![bin_executable]);

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn falls_back_to_root_when_bin_directory_is_empty() {
        let root = temp_root("empty-bin");
        fs::create_dir_all(root.join("bin")).expect("create bin");
        let root_executable = root.join(executable_name("tool"));
        fs::write(&root_executable, b"#!/bin/sh\n").expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root_executable, fs::Permissions::from_mode(0o755))
                .expect("make executable");
        }

        assert_eq!(find_executables(&root, "tool"), vec![root_executable]);

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn ignores_executable_shared_libraries() {
        let root = temp_root("libraries");
        fs::create_dir_all(&root).expect("create root");
        let command = root.join(executable_name("tool"));
        let shared_objects = [root.join("libtool.so"), root.join("helper.dll")];

        for path in std::iter::once(&command).chain(shared_objects.iter()) {
            fs::write(path, b"not a real executable").expect("write file");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                    .expect("make executable");
            }
        }

        assert_eq!(find_executables(&root, "tool"), vec![command]);

        cleanup(&root).expect("cleanup");
    }
}
