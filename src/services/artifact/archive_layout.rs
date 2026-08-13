use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    models::upstream::Package,
    services::artifact::permission_handler,
    utils::{
        filename_parser::{parse_arch, parse_os},
        platform::platform_info::{ArchitectureInfo, CpuArch, OSKind},
    },
};

/// Choose an extracted subdirectory only when its name explicitly identifies
/// a compatible platform and it contains the package executable.
pub fn select_nested_archive_root(extracted_path: &Path, package: &Package) -> Option<PathBuf> {
    if !extracted_path.is_dir() {
        return None;
    }

    let architecture = ArchitectureInfo::new();
    let mut candidates = fs::read_dir(extracted_path)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let target_os = parse_os(&name)?;
            let target_arch = parse_arch(&name)?;
            if target_os != architecture.os_kind {
                return None;
            }

            let lower = name.to_ascii_lowercase();
            if package
                .exclude_pattern
                .as_slice()
                .iter()
                .any(|pattern| lower.contains(pattern))
            {
                return None;
            }

            let arch_score = nested_arch_score(&architecture.cpu_arch, &target_arch)?;
            permission_handler::find_executable(&entry.path(), &package.name)?;
            let score = nested_archive_score(&name, &target_os, arch_score, &package.match_pattern);
            Some((score, name, entry.path()))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    candidates.into_iter().next().map(|(_, _, path)| path)
}

fn nested_arch_score(host_arch: &CpuArch, target_arch: &CpuArch) -> Option<i32> {
    if host_arch == target_arch {
        return Some(100);
    }

    if (*host_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86)
        || (*host_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm)
    {
        return Some(40);
    }

    None
}

fn nested_archive_score(
    name: &str,
    target_os: &OSKind,
    arch_score: i32,
    match_pattern: &crate::providers::pattern_matcher::PatternTable,
) -> i32 {
    let mut score = arch_score;
    let lower = name.to_ascii_lowercase();
    if *target_os == OSKind::Linux {
        score += linux_abi_score(&lower);
    }

    if !match_pattern.is_empty() {
        score += (match_pattern.match_ratio(&lower) * 100.0).round() as i32;
    }

    score
}

fn linux_abi_score(name: &str) -> i32 {
    #[cfg(all(target_os = "linux", target_env = "musl"))]
    {
        if name.contains("musl") {
            30
        } else if name.contains("gnu") || name.contains("glibc") {
            10
        } else {
            0
        }
    }

    #[cfg(all(target_os = "linux", not(target_env = "musl")))]
    {
        if name.contains("linux-gnu") && !name.contains("glibc") {
            30
        } else if name.contains("glibc") {
            20
        } else if name.contains("musl") {
            10
        } else {
            0
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        0
    }
}
