use crate::models::common::enums::Filetype;
use crate::models::provider::Asset;
use crate::utils::platform::platform_info::{ArchitectureInfo, CpuArch};

pub(in crate::providers::assets) fn supported_filetypes_for_os() -> Vec<Filetype> {
    #[cfg(target_os = "linux")]
    return vec![
        Filetype::AppImage,
        Filetype::Archive,
        Filetype::Compressed,
        Filetype::Binary,
    ];

    #[cfg(target_os = "windows")]
    return vec![Filetype::WinExe, Filetype::Archive, Filetype::Compressed];

    #[cfg(target_os = "macos")]
    return vec![Filetype::Archive, Filetype::Compressed, Filetype::Binary];
}

pub(in crate::providers::assets) fn is_potentially_compatible(
    asset: &Asset,
    architecture: &ArchitectureInfo,
) -> bool {
    if let Some(target_os) = &asset.target_os
        && *target_os != architecture.os_kind
    {
        return false;
    }

    match &asset.target_arch {
        None => true,
        Some(target_arch) if *target_arch == architecture.cpu_arch => true,
        Some(CpuArch::X86) if architecture.cpu_arch == CpuArch::X86_64 => true,
        Some(CpuArch::Arm) if architecture.cpu_arch == CpuArch::Aarch64 => true,
        Some(_) => false,
    }
}

pub(super) fn target_score(asset: &Asset, architecture: &ArchitectureInfo) -> i32 {
    let mut score = 0;

    if asset.target_os.as_ref() == Some(&architecture.os_kind) {
        score += 80;
    }

    if let Some(target_arch) = &asset.target_arch {
        if *target_arch == architecture.cpu_arch {
            score += 80;
        } else if (*target_arch == CpuArch::X86 && architecture.cpu_arch == CpuArch::X86_64)
            || (*target_arch == CpuArch::Arm && architecture.cpu_arch == CpuArch::Aarch64)
        {
            score += 30;
        }
    }

    score
}
