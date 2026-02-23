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
