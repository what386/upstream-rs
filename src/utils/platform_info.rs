use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OSKind {
    Windows,
    MacOS,
    Linux,
    FreeBSD,
    OpenBSD,
    NetBSD,
    Android,
    IOS,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuArch {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Ppc,
    Ppc64,
    Riscv64,
    S390x,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCpuArchError;

impl fmt::Display for ParseCpuArchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse CPU architecture")
    }
}

impl std::error::Error for ParseCpuArchError {}

impl FromStr for CpuArch {
    type Err = ParseCpuArchError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x86" => Ok(Self::X86),
            "x86_64" => Ok(Self::X86_64),
            "arm" => Ok(Self::Arm),
            "aarch64" => Ok(Self::Aarch64),
            "powerpc" => Ok(Self::Ppc),
            "powerpc64" => Ok(Self::Ppc64),
            "riscv64" => Ok(Self::Riscv64),
            "s390x" => Ok(Self::S390x),
            _ => Ok(Self::Unknown),
        }
    }
}

impl FromStr for OSKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "windows" => Ok(Self::Windows),
            "macos" => Ok(Self::MacOS),
            "linux" => Ok(Self::Linux),
            "freebsd" => Ok(Self::FreeBSD),
            "openbsd" => Ok(Self::OpenBSD),
            "netbsd" => Ok(Self::NetBSD),
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::IOS),
            _ => Ok(Self::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureInfo {
    pub is_os_64_bit: bool,
    pub cpu_arch: CpuArch,
    pub os_kind: OSKind,
}

impl ArchitectureInfo {
    pub fn new_compiletime() -> Self {
        let is_os_64_bit = cfg!(target_pointer_width = "64");

        #[cfg(target_arch = "x86")]
        let cpu_arch = CpuArch::X86;
        #[cfg(target_arch = "x86_64")]
        let cpu_arch = CpuArch::X86_64;
        #[cfg(target_arch = "arm")]
        let cpu_arch = CpuArch::Arm;
        #[cfg(target_arch = "aarch64")]
        let cpu_arch = CpuArch::Aarch64;
        #[cfg(target_arch = "powerpc")]
        let cpu_arch = CpuArch::Ppc;
        #[cfg(target_arch = "powerpc64")]
        let cpu_arch = CpuArch::Ppc64;
        #[cfg(target_arch = "riscv64")]
        let cpu_arch = CpuArch::Riscv64;
        #[cfg(target_arch = "s390x")]
        let cpu_arch = CpuArch::S390x;
        #[cfg(not(any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "powerpc",
            target_arch = "powerpc64",
            target_arch = "riscv64",
            target_arch = "s390x"
        )))]
        let cpu_arch = CpuArch::Unknown;

        #[cfg(target_os = "windows")]
        let os_kind = OSKind::Windows;
        #[cfg(target_os = "macos")]
        let os_kind = OSKind::MacOS;
        #[cfg(target_os = "linux")]
        let os_kind = OSKind::Linux;
        #[cfg(target_os = "freebsd")]
        let os_kind = OSKind::FreeBSD;
        #[cfg(target_os = "openbsd")]
        let os_kind = OSKind::OpenBSD;
        #[cfg(target_os = "netbsd")]
        let os_kind = OSKind::NetBSD;
        #[cfg(target_os = "android")]
        let os_kind = OSKind::Android;
        #[cfg(target_os = "ios")]
        let os_kind = OSKind::IOS;
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "android",
            target_os = "ios"
        )))]
        let os_kind = OSKind::Unknown;

        Self {
            is_os_64_bit,
            cpu_arch,
            os_kind,
        }
    }

    pub fn new() -> Self {
        Self::new_compiletime()
    }
}

impl Default for ArchitectureInfo {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_arch(arch: &CpuArch) -> &str {
    match arch {
        CpuArch::X86 => "x86",
        CpuArch::X86_64 => "x86_64",
        CpuArch::Arm => "ARM",
        CpuArch::Aarch64 => "ARM64",
        CpuArch::Ppc => "PowerPC",
        CpuArch::Ppc64 => "PowerPC64",
        CpuArch::Riscv64 => "RISC-V 64",
        CpuArch::S390x => "s390x",
        CpuArch::Unknown => "Unknown",
    }
}

pub fn format_os(os: &OSKind) -> &str {
    match os {
        OSKind::Windows => "Windows",
        OSKind::MacOS => "macOS",
        OSKind::Linux => "Linux",
        OSKind::FreeBSD => "FreeBSD",
        OSKind::OpenBSD => "OpenBSD",
        OSKind::NetBSD => "NetBSD",
        OSKind::Android => "Android",
        OSKind::IOS => "iOS",
        OSKind::Unknown => "Unknown OS",
    }
}
