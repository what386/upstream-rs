//! Discover executable files and rank fallback commands in extracted artifacts.

#[cfg(unix)]
use std::io::Read;
use std::{
    fs,
    path::{Path, PathBuf},
};

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

    sort_executables(&mut paths, preferred_name);
    paths
}

fn sort_executables(paths: &mut [PathBuf], preferred_name: &str) {
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
        || path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".so."))
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

/// Recovery candidates when permission-based discovery finds nothing. Inspect
/// both supported locations, without following symlinks or executing payloads.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutableCandidate {
    pub path: PathBuf,
    pub score: u8,
}

/// Scores rank evidence, rather than representing a probability: executable
/// permissions 90, a native binary header 80, or a script shebang 60. An exact
/// match with the preferred command name adds 10 points.
pub fn find_executable_candidates(
    directory: &Path,
    preferred_name: &str,
) -> Vec<ExecutableCandidate> {
    let mut paths = Vec::new();
    for directory in [directory.join("bin"), directory.to_path_buf()] {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_file()) && !is_shared_library(&path) {
                let score = if is_executable_file(&path) {
                    Some(90)
                } else {
                    executable_header_score(&path)
                };
                if let Some(mut score) = score {
                    if path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case(preferred_name))
                    {
                        score += 10;
                    }
                    paths.push(ExecutableCandidate { path, score });
                }
            }
        }
    }
    paths.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
    });
    paths
}

#[cfg(windows)]
fn executable_header_score(_path: &Path) -> Option<u8> {
    None
}

#[cfg(unix)]
fn executable_header_score(path: &Path) -> Option<u8> {
    let Ok(mut file) = fs::File::open(path) else {
        return None;
    };
    let mut header = [0_u8; 64];
    // A short file is valid input (for example, a small shell script).
    let mut read = 0;
    while read < header.len() {
        match file.read(&mut header[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(_) => return None,
        }
    }
    let header = &header[..read];

    #[cfg(unix)]
    if header.starts_with(b"#!/") || header.starts_with(b"#! /") {
        return Some(60);
    }

    #[cfg(target_os = "linux")]
    if header.len() >= 52 && header.starts_with(b"\x7fELF") {
        let kind = match header[5] {
            1 => u16::from_le_bytes([header[16], header[17]]),
            2 => u16::from_be_bytes([header[16], header[17]]),
            _ => return None,
        };
        let entry = match header[4] {
            1 => &header[24..28],
            2 if header.len() >= 64 => &header[24..32],
            _ => return None,
        };
        // ET_EXEC, or a PIE (ET_DYN with an entry point). Shared libraries
        // normally have no entry point and must not become commands.
        return (kind == 2 || (kind == 3 && entry.iter().any(|byte| *byte != 0))).then_some(80);
    }

    #[cfg(target_os = "macos")]
    if header.len() >= 28 {
        let kind = match &header[..4] {
            [0xce, 0xfa, 0xed, 0xfe] | [0xcf, 0xfa, 0xed, 0xfe] => {
                u32::from_le_bytes(header[12..16].try_into().unwrap())
            }
            [0xfe, 0xed, 0xfa, 0xce] | [0xfe, 0xed, 0xfa, 0xcf] => {
                u32::from_be_bytes(header[12..16].try_into().unwrap())
            }
            _ => return None,
        };
        return (kind == 2).then_some(80); // MH_EXECUTE
    }

    None
}

#[cfg(test)]
mod tests {
    use super::find_executables;
    #[cfg(unix)]
    use super::{ExecutableCandidate, find_executable_candidates};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn temp_root(name: &str) -> PathBuf {
        crate::utils::test_support::temp_root("upstream-executable-candidates", name)
    }

    fn cleanup(path: &Path) -> std::io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[cfg(unix)]
    #[test]
    fn fallback_finds_scripts_without_execute_bits_in_root_and_bin() {
        let root = temp_root("fallback-scripts");
        fs::create_dir_all(root.join("bin")).unwrap();
        let command = root.join("tool");
        let helper = root.join("bin/helper");
        for path in [&command, &helper] {
            fs::write(path, b"#!/bin/sh\necho hello\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(root.join("README"), b"Use tool to run this program").unwrap();
        fs::write(root.join("libtool.so.1"), b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&command, root.join("tool-link")).unwrap();
        assert!(find_executables(&root, "tool").is_empty());
        assert_eq!(
            find_executable_candidates(&root, "tool"),
            vec![
                ExecutableCandidate {
                    path: command,
                    score: 70
                },
                ExecutableCandidate {
                    path: helper,
                    score: 60
                }
            ]
        );
        cleanup(&root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fallback_recognizes_elf_commands_but_rejects_objects_and_libraries() {
        let root = temp_root("fallback-elf");
        fs::create_dir_all(&root).unwrap();
        let mut header = [0_u8; 64];
        header[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        for (name, kind, entry) in [
            ("bw", 2, 1),
            ("pie", 3, 1),
            ("library", 3, 0),
            ("object", 1, 0),
        ] {
            header[16] = kind;
            header[24] = entry;
            let path = root.join(name);
            fs::write(&path, header).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(root.join("truncated"), b"\x7fELF").unwrap();
        assert!(find_executables(&root, "cli").is_empty());
        assert_eq!(
            find_executable_candidates(&root, "cli"),
            vec![
                ExecutableCandidate {
                    path: root.join("bw"),
                    score: 80
                },
                ExecutableCandidate {
                    path: root.join("pie"),
                    score: 80
                }
            ]
        );
        // A preferred name breaks ties without depending on directory order.
        let preferred = find_executable_candidates(&root, "pie");
        assert_eq!(preferred[0].path, root.join("pie"));
        assert_eq!(preferred[0].score, 90);
        cleanup(&root).unwrap();
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
