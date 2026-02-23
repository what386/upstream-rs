use super::{ArchitectureInfo, CpuArch, OSKind, format_arch, format_os};
use std::str::FromStr;

#[test]
fn cpu_arch_from_str_maps_known_and_unknown_values() {
    assert_eq!(CpuArch::from_str("x86").expect("x86 parse"), CpuArch::X86);
    assert_eq!(
        CpuArch::from_str("aarch64").expect("aarch64 parse"),
        CpuArch::Aarch64
    );
    assert_eq!(
        CpuArch::from_str("something-else").expect("unknown parse"),
        CpuArch::Unknown
    );
}

#[test]
fn os_kind_from_str_maps_known_and_unknown_values() {
    assert_eq!(
        OSKind::from_str("windows").expect("windows parse"),
        OSKind::Windows
    );
    assert_eq!(
        OSKind::from_str("linux").expect("linux parse"),
        OSKind::Linux
    );
    assert_eq!(
        OSKind::from_str("weird-os").expect("unknown parse"),
        OSKind::Unknown
    );
}

#[test]
fn formatter_outputs_are_stable() {
    assert_eq!(format_arch(&CpuArch::X86_64), "x86_64");
    assert_eq!(format_arch(&CpuArch::Unknown), "Unknown");
    assert_eq!(format_os(&OSKind::MacOS), "macOS");
    assert_eq!(format_os(&OSKind::Unknown), "Unknown OS");
}

#[test]
fn architecture_info_default_constructs_reasonable_values() {
    let info = ArchitectureInfo::default();
    assert!(!format_arch(&info.cpu_arch).is_empty());
    assert!(!format_os(&info.os_kind).is_empty());
}
