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

/// Finds any potential executables in a directory.
pub fn find_executable(directory_path: &Path, name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let name = &format!("{}.exe", name);

    // 1. bin/<name>
    let bin_path = directory_path.join("bin").join(name);
    if bin_path.is_file() {
        return Some(bin_path);
    }

    // 2. directoryPath/<name>
    let direct_path = directory_path.join(name);
    if direct_path.is_file() {
        return Some(direct_path);
    }

    // 3. directory name is the executable name
    //    e.g. cool-app-x86_64/cool-app-x86_64
    if let Some(dir_name) = directory_path.file_name() {
        let derived_path = directory_path.join(dir_name);
        if derived_path.is_file() {
            return Some(derived_path);
        }
    }

    // 4. As a fallback, search for any file starting with name
    //    e.g. "cool-app" -> "cool-app-x86_64", "cool-app-v1"
    if let Ok(entries) = fs::read_dir(directory_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_file()
                && let Some(file_name) = entry.file_name().to_str()
                && file_name.to_lowercase().starts_with(&name.to_lowercase())
            {
                return Some(entry.path());
            }
        }
    }

    // 5. Handle nested layouts such as "<tool>-linux/<arch>/<tool>".
    if let Some(path) = find_nested_executable(directory_path, name, true) {
        return Some(path);
    }

    // 6. Final fallback: any nested exact-name match up to limited depth.
    if let Some(path) = find_nested_executable(directory_path, name, false) {
        return Some(path);
    }

    None
}

fn find_nested_executable(root: &Path, name: &str, prefer_arch_paths: bool) -> Option<PathBuf> {
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let arch_markers = current_arch_markers();
    let target_name = name.to_ascii_lowercase();

    while let Some((dir, depth)) = stack.pop() {
        if depth > 3 {
            continue;
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if file_name.to_ascii_lowercase() != target_name {
                continue;
            }

            if !prefer_arch_paths {
                return Some(path);
            }

            let contains_arch_marker = path.components().any(|component| {
                let s = component.as_os_str().to_string_lossy().to_ascii_lowercase();
                arch_markers.iter().any(|marker| s == *marker)
            });

            if contains_arch_marker {
                return Some(path);
            }
        }
    }

    None
}

fn current_arch_markers() -> &'static [&'static str] {
    #[cfg(target_arch = "x86_64")]
    return &["x86_64", "amd64", "x64"];
    #[cfg(target_arch = "x86")]
    return &["x86", "i386", "i686", "x86_32"];
    #[cfg(target_arch = "aarch64")]
    return &["aarch64", "arm64"];
    #[cfg(target_arch = "arm")]
    return &["arm", "armv7", "armv6"];
    #[cfg(target_arch = "riscv64")]
    return &["riscv64"];
    #[cfg(target_arch = "powerpc")]
    return &["powerpc", "ppc"];
    #[cfg(target_arch = "powerpc64")]
    return &["powerpc64", "ppc64"];
    #[cfg(target_arch = "s390x")]
    return &["s390x"];
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "s390x"
    )))]
    return &[];
}

#[cfg(test)]
mod tests {
    use super::find_executable;
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

    #[test]
    fn finds_nested_arch_layout_executable() {
        let root = temp_root("nested-arch");
        let install_root = root.join("minisign-0.12-linux");
        let arch_dir = install_root.join("minisign-linux").join("x86_64");
        fs::create_dir_all(&arch_dir).expect("create nested dirs");
        fs::write(arch_dir.join("minisign"), b"#!/bin/sh\n").expect("write executable");

        let found = find_executable(&install_root, "minisign").expect("find executable");
        assert!(found.ends_with("minisign-linux/x86_64/minisign"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn still_finds_direct_binary_first() {
        let root = temp_root("direct");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("tool"), b"#!/bin/sh\n").expect("write executable");

        let found = find_executable(&root, "tool").expect("find executable");
        assert!(found.ends_with("tool"));

        cleanup(&root).expect("cleanup");
    }
}
