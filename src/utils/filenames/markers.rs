pub const WINDOWS: &[&str] = &[
    ".exe", ".msi", ".dll", "windows", "win64", "win32", "win", "msvc", ".nsis",
];

pub const IOS: &[&str] = &["ios", "iphone", "ipad"];
pub const MACOS: &[&str] = &["macos", "darwin", "osx", "mac"];
pub const ANDROID: &[&str] = &["android", ".apk", ".aab"];
pub const LINUX: &[&str] = &["linux", "gnu", ".appimage", "musl"];
pub const FREEBSD: &[&str] = &["freebsd", "fbsd"];
pub const OPENBSD: &[&str] = &["openbsd", "obsd"];
pub const NETBSD: &[&str] = &["netbsd", "nbsd"];

pub const AARCH64: &[&str] = &["aarch64", "arm64", "armv8"];
pub const ARM: &[&str] = &["armv7", "armv7l", "armv6", "arm"];
pub const X86_64: &[&str] = &["x86_64", "x86-64", "amd64", "x64", "win64"];
pub const X86: &[&str] = &["x86_32", "x86-32", "win32", "x86"];

pub fn contains_platform_marker(filename: &str) -> bool {
    [
        WINDOWS, IOS, MACOS, ANDROID, LINUX, FREEBSD, OPENBSD, NETBSD, AARCH64, ARM, X86_64, X86,
    ]
    .into_iter()
    .any(|markers| contains_marker(filename, markers))
}

/// Match token markers with word-boundary checks to reduce false positives.
/// Extension markers (starting with `.`) are treated as suffix matches.
pub fn contains_marker(filename: &str, markers: &[&str]) -> bool {
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
