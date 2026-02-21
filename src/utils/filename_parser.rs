use crate::models::common::enums::Filetype;
use crate::utils::platform_info::{CpuArch, OSKind};

const ARCHIVE_EXTENSIONS: &[&str] = &[
    ".zip", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tbz", ".tar.xz", ".txz", ".7z",
    ".rar", ".tar.zst", ".tzst",
];

const COMPRESSION_EXTENSIONS: &[&str] = &[".gz", ".br", ".bz2", ".zst"];

const CHECKSUM_EXTENSIONS: &[&str] = &[
    ".sha256", ".sha512", ".sha1", ".md5", ".sig", ".asc", ".minisig", ".sum",
];

pub fn parse_os(filename: &str) -> Option<OSKind> {
    let name = filename.to_lowercase();

    // Windows
    if contains_marker(
        &name,
        &[
            ".exe", ".msi", ".dll", "windows", "win64", "win32", "win", "msvc",
        ],
    ) {
        return Some(OSKind::Windows);
    }

    // iOS
    if contains_marker(&name, &["ios", "iphone", "ipad"]) {
        return Some(OSKind::Ios);
    }

    // macOS/Darwin
    if contains_marker(&name, &["macos", "darwin", "osx", "mac", ".dmg", ".app"]) {
        return Some(OSKind::MacOS);
    }

    // Android
    if contains_marker(&name, &["android", ".apk", ".aab"]) {
        return Some(OSKind::Android);
    }

    // Linux
    if contains_marker(&name, &["linux", "gnu", ".appimage", "musl"]) {
        return Some(OSKind::Linux);
    }

    // FreeBSD
    if contains_marker(&name, &["freebsd", "fbsd"]) {
        return Some(OSKind::FreeBSD);
    }

    // OpenBSD
    if contains_marker(&name, &["openbsd", "obsd"]) {
        return Some(OSKind::OpenBSD);
    }

    // NetBSD
    if contains_marker(&name, &["netbsd", "nbsd"]) {
        return Some(OSKind::NetBSD);
    }

    None
}

pub fn parse_arch(filename: &str) -> Option<CpuArch> {
    let name = filename.to_lowercase();

    if contains_marker(&name, &["aarch64", "arm64", "armv8"]) {
        return Some(CpuArch::Aarch64);
    }

    if contains_marker(&name, &["armv7", "armv7l", "armv6", "arm"]) {
        return Some(CpuArch::Arm);
    }

    if contains_marker(&name, &["x86_64", "x86-64", "amd64", "x64", "win64"]) {
        return Some(CpuArch::X86_64);
    }

    if contains_marker(&name, &["x86_32", "x86-32", "win32"]) {
        return Some(CpuArch::X86);
    }

    // Ambiguous "x86"
    if contains_marker(&name, &["x86"]) {
        if name.contains("32") {
            return Some(CpuArch::X86);
        }
        // Default ambiguous x86 to 64-bit
        return Some(CpuArch::X86_64);
    }

    None
}

pub fn parse_filetype(filename: &str) -> Filetype {
    let filename = filename.to_lowercase();

    if filename.ends_with(".appimage") {
        return Filetype::AppImage;
    }

    if filename.ends_with(".exe") {
        return Filetype::WinExe;
    }

    if ARCHIVE_EXTENSIONS.iter().any(|ext| filename.ends_with(ext)) {
        return Filetype::Archive;
    }

    if COMPRESSION_EXTENSIONS
        .iter()
        .any(|ext| filename.ends_with(ext))
    {
        return Filetype::Compressed;
    }

    if CHECKSUM_EXTENSIONS
        .iter()
        .any(|ext| filename.ends_with(ext))
    {
        return Filetype::Checksum;
    }

    Filetype::Binary
}

fn contains_marker(filename: &str, markers: &[&str]) -> bool {
    for marker in markers {
        if marker.starts_with('.') {
            if filename.ends_with(marker) {
                return true;
            }
            continue;
        }

        if let Some(mut index) = filename.find(marker) {
            loop {
                let valid_start =
                    index == 0 || !filename.chars().nth(index - 1).unwrap().is_alphanumeric();

                let valid_end = index + marker.len() >= filename.len()
                    || !filename
                        .chars()
                        .nth(index + marker.len())
                        .unwrap()
                        .is_alphanumeric();

                if valid_start && valid_end {
                    return true;
                }

                // Find next occurrence
                match filename[index + 1..].find(marker) {
                    Some(offset) => index = index + 1 + offset,
                    None => break,
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{parse_arch, parse_filetype, parse_os};
    use crate::models::common::enums::Filetype;
    use crate::utils::platform_info::{CpuArch, OSKind};

    #[test]
    fn parse_os_detects_expected_platforms() {
        assert_eq!(parse_os("tool-windows-x64.zip"), Some(OSKind::Windows));
        assert_eq!(parse_os("tool-macos-universal.dmg"), Some(OSKind::MacOS));
        assert_eq!(parse_os("tool-linux-musl.tar.gz"), Some(OSKind::Linux));
        assert_eq!(parse_os("app-android-arm64.apk"), Some(OSKind::Android));
    }

    #[test]
    fn parse_os_respects_marker_boundaries() {
        assert_eq!(parse_os("darwinia-release.tar.gz"), None);
        assert_eq!(parse_os("twindow-package.tar.gz"), None);
    }

    #[test]
    fn parse_arch_detects_common_variants() {
        assert_eq!(parse_arch("tool-aarch64.tar.gz"), Some(CpuArch::Aarch64));
        assert_eq!(parse_arch("tool-armv7.tar.gz"), Some(CpuArch::Arm));
        assert_eq!(parse_arch("tool-amd64.zip"), Some(CpuArch::X86_64));
        assert_eq!(parse_arch("tool-x86_32.zip"), Some(CpuArch::X86));
    }

    #[test]
    fn parse_arch_defaults_ambiguous_x86_to_64_bit() {
        assert_eq!(parse_arch("tool-x86.zip"), Some(CpuArch::X86_64));
        assert_eq!(parse_arch("tool-x86-32.zip"), Some(CpuArch::X86));
    }

    #[test]
    fn parse_filetype_classifies_extensions_in_priority_order() {
        assert_eq!(parse_filetype("tool.AppImage"), Filetype::AppImage);
        assert_eq!(parse_filetype("tool.exe"), Filetype::WinExe);
        assert_eq!(parse_filetype("tool.tar.gz"), Filetype::Archive);
        assert_eq!(parse_filetype("tool.gz"), Filetype::Compressed);
        assert_eq!(parse_filetype("tool.sha256"), Filetype::Checksum);
        assert_eq!(parse_filetype("tool"), Filetype::Binary);
    }
}
