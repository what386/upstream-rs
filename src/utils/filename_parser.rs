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

/// Infer target OS from common release artifact naming markers.
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

/// Infer CPU architecture from artifact naming conventions.
///
/// Ambiguous `x86` markers default to `X86_64` unless a 32-bit marker is also present.
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
        return Some(CpuArch::X86);
    }

    None
}

/// Classify an artifact into upstream's installable file categories.
///
/// Detection is extension-based and ordered from most specific to most general.
pub fn parse_filetype(filename: &str) -> Filetype {
    let filename = filename.to_lowercase();

    if filename.ends_with(".appimage") {
        return Filetype::AppImage;
    }

    if filename.ends_with(".app") {
        return Filetype::MacApp;
    }

    if filename.ends_with(".dmg") {
        return Filetype::MacDmg;
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

/// Match token markers with word-boundary checks to reduce false positives.
///
/// Extension markers (starting with `.`) are treated as suffix matches.
fn contains_marker(filename: &str, markers: &[&str]) -> bool {
    for marker in markers {
        if marker.starts_with('.') {
            if filename.ends_with(marker) {
                return true;
            }
            continue;
        }

        let mut search_start = 0usize;
        while let Some(offset) = filename[search_start..].find(marker) {
            let index = search_start + offset;
            let bytes = filename.as_bytes();
            let marker_end = index + marker.len();

            let valid_start = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
            let valid_end = marker_end >= bytes.len() || !bytes[marker_end].is_ascii_alphanumeric();

            if valid_start && valid_end {
                return true;
            }

            search_start = index + 1;
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
    fn parse_arch_defaults_ambiguous_x86_to_32_bit() {
        assert_eq!(parse_arch("tool-x86.zip"), Some(CpuArch::X86));
        assert_eq!(parse_arch("tool-x86-32.zip"), Some(CpuArch::X86));
    }

    #[test]
    fn parse_filetype_classifies_extensions_in_priority_order() {
        assert_eq!(parse_filetype("tool.AppImage"), Filetype::AppImage);
        assert_eq!(parse_filetype("tool.app"), Filetype::MacApp);
        assert_eq!(parse_filetype("tool.dmg"), Filetype::MacDmg);
        assert_eq!(parse_filetype("tool.exe"), Filetype::WinExe);
        assert_eq!(parse_filetype("tool.tar.gz"), Filetype::Archive);
        assert_eq!(parse_filetype("tool.gz"), Filetype::Compressed);
        assert_eq!(parse_filetype("tool.sha256"), Filetype::Checksum);
        assert_eq!(parse_filetype("tool"), Filetype::Binary);
    }
}
