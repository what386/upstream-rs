use super::markers;
use crate::models::common::enums::Filetype;
use crate::utils::platform::platform_info::{CpuArch, OSKind};

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
    if markers::contains_marker(&name, markers::WINDOWS) {
        return Some(OSKind::Windows);
    }

    // iOS
    if markers::contains_marker(&name, markers::IOS) {
        return Some(OSKind::Ios);
    }

    // macOS/Darwin
    if markers::contains_marker(&name, markers::MACOS) {
        return Some(OSKind::MacOS);
    }

    // Android
    if markers::contains_marker(&name, markers::ANDROID) {
        return Some(OSKind::Android);
    }

    // Linux
    if markers::contains_marker(&name, markers::LINUX) {
        return Some(OSKind::Linux);
    }

    // FreeBSD
    if markers::contains_marker(&name, markers::FREEBSD) {
        return Some(OSKind::FreeBSD);
    }

    // OpenBSD
    if markers::contains_marker(&name, markers::OPENBSD) {
        return Some(OSKind::OpenBSD);
    }

    // NetBSD
    if markers::contains_marker(&name, markers::NETBSD) {
        return Some(OSKind::NetBSD);
    }

    None
}

/// Infer CPU architecture from artifact naming conventions.
///
/// Ambiguous `x86` markers default to `X86_64` unless a 32-bit marker is also present.
pub fn parse_arch(filename: &str) -> Option<CpuArch> {
    let name = filename.to_lowercase();

    if markers::contains_marker(&name, markers::AARCH64) {
        return Some(CpuArch::Aarch64);
    }

    if markers::contains_marker(&name, markers::ARM) {
        return Some(CpuArch::Arm);
    }

    if markers::contains_marker(&name, markers::X86_64) {
        return Some(CpuArch::X86_64);
    }

    if markers::contains_marker(&name, &markers::X86[..3]) {
        return Some(CpuArch::X86);
    }

    // Ambiguous "x86"
    if markers::contains_marker(&name, &markers::X86[3..]) {
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

/// Returns whether an artifact format is not supported by the installer.
pub fn is_unsupported_artifact_name(filename: &str) -> bool {
    let filename = filename.to_ascii_lowercase();
    [
        ".app",
        ".dmg",
        ".deb",
        ".rpm",
        ".apk",
        ".pkg.tar.zst",
        ".pkg.tar.xz",
        ".pkg.tar.gz",
        ".pkg.tar",
        ".pacman",
        ".flatpak",
        ".snap",
    ]
    .iter()
    .any(|extension| filename.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::{is_unsupported_artifact_name, parse_arch, parse_filetype, parse_os};
    use crate::models::common::enums::Filetype;
    use crate::utils::platform::platform_info::{CpuArch, OSKind};

    #[test]
    fn parse_os_detects_expected_platforms() {
        assert_eq!(parse_os("tool-windows-x64.zip"), Some(OSKind::Windows));
        assert_eq!(parse_os("tool-macos-universal.tar.gz"), Some(OSKind::MacOS));
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
        assert_eq!(parse_filetype("tool.exe"), Filetype::WinExe);
        assert_eq!(parse_filetype("tool.tar.gz"), Filetype::Archive);
        assert_eq!(parse_filetype("tool.gz"), Filetype::Compressed);
        assert_eq!(parse_filetype("tool.sha256"), Filetype::Checksum);
        assert_eq!(parse_filetype("tool"), Filetype::Binary);
    }

    #[test]
    fn unsupported_desktop_artifacts_are_rejected_by_name() {
        assert!(is_unsupported_artifact_name("Tool.app"));
        assert!(is_unsupported_artifact_name("Tool.dmg"));
        assert!(is_unsupported_artifact_name("tool.deb"));
        assert!(is_unsupported_artifact_name("tool.rpm"));
        assert!(is_unsupported_artifact_name("tool.pkg.tar.zst"));
        assert!(is_unsupported_artifact_name("tool.flatpak"));
        assert!(!is_unsupported_artifact_name("Tool.AppImage"));
    }
}
