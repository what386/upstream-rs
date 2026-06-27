use std::path::Path;

use anyhow::{Result, anyhow};

use crate::models::common::enums::Filetype;
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::asset_selector::AssetCandidate;
use crate::providers::pattern_matcher::pattern_match_ratio;
use crate::utils::math::median_sorted;
use crate::utils::platform::platform_info::{ArchitectureInfo, CpuArch, format_arch, format_os};

#[derive(Debug, Clone)]
pub(crate) struct HeuristicAssetSelector {
    architecture_info: ArchitectureInfo,
}

#[derive(Debug, Clone, Copy)]
struct AssetSizeProfile {
    median: u64,
}

impl HeuristicAssetSelector {
    pub(crate) fn new(architecture_info: ArchitectureInfo) -> Self {
        Self { architecture_info }
    }

    pub(crate) fn find_recommended_asset(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Asset> {
        let target_filetype = if package.filetype == Filetype::Auto {
            Self::resolve_auto_filetype(release)?
        } else {
            package.filetype
        };

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| a.filetype == target_filetype)
            .collect();
        let size_profile = AssetSizeProfile::from_assets(compatible_assets.iter().copied());

        compatible_assets
            .into_iter()
            .max_by_key(|a| self.score_asset(a, package, size_profile.as_ref()))
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "No compatible assets found for {} on {}",
                    format_arch(&self.architecture_info.cpu_arch),
                    format_os(&self.architecture_info.os_kind)
                )
            })
    }

    pub(crate) fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        let target_filetype = if package.filetype == Filetype::Auto {
            Self::resolve_auto_filetype(release)?
        } else {
            package.filetype
        };

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| a.filetype == target_filetype)
            .collect();
        let size_profile = AssetSizeProfile::from_assets(compatible_assets.iter().copied());

        let mut candidates: Vec<AssetCandidate> = compatible_assets
            .into_iter()
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: self.score_asset(asset, package, size_profile.as_ref()),
            })
            .collect();

        candidates.sort_by_key(|b| std::cmp::Reverse(b.score));
        Ok(candidates)
    }

    pub(crate) fn get_installable_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Vec<AssetCandidate> {
        let target_filetypes = self.target_filetypes(package.filetype);

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| target_filetypes.contains(&a.filetype))
            .collect();
        let size_profile = AssetSizeProfile::from_assets(compatible_assets.iter().copied());

        let mut candidates: Vec<AssetCandidate> = compatible_assets
            .into_iter()
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: self.score_asset(asset, package, size_profile.as_ref()),
            })
            .collect();

        candidates.sort_by_key(|b| std::cmp::Reverse(b.score));
        candidates
    }

    pub(crate) fn target_filetypes(&self, filetype: Filetype) -> Vec<Filetype> {
        if filetype == Filetype::Auto {
            Self::get_priority_for_os()
        } else {
            vec![filetype]
        }
    }

    pub(crate) fn resolve_auto_filetype(release: &Release) -> Result<Filetype> {
        let priority = Self::get_priority_for_os();

        priority
            .iter()
            .find(|&&filetype| {
                release
                    .assets
                    .iter()
                    .any(|asset| asset.filetype == filetype)
            })
            .copied()
            .ok_or_else(|| anyhow!("No compatible filetype found in release assets"))
    }

    pub(crate) fn is_potentially_compatible(&self, asset: &Asset) -> bool {
        if let Some(target_os) = &asset.target_os
            && *target_os != self.architecture_info.os_kind
        {
            return false;
        }

        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                return true;
            }

            if self.architecture_info.cpu_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86 {
                return true;
            }

            if self.architecture_info.cpu_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm {
                return true;
            }

            return *target_arch == self.architecture_info.cpu_arch;
        }

        true
    }

    fn get_priority_for_os() -> Vec<Filetype> {
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
        return vec![
            Filetype::MacApp,
            Filetype::MacDmg,
            Filetype::Archive,
            Filetype::Compressed,
            Filetype::Binary,
        ];
    }

    fn score_asset(
        &self,
        asset: &Asset,
        package: &Package,
        size_profile: Option<&AssetSizeProfile>,
    ) -> i32 {
        let name = asset.name.to_lowercase();
        let mut score = 0;

        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                score += 80;
            } else if (self.architecture_info.cpu_arch == CpuArch::X86_64
                && *target_arch == CpuArch::X86)
                || (self.architecture_info.cpu_arch == CpuArch::Aarch64
                    && *target_arch == CpuArch::Arm)
            {
                score += 30;
            }
        }

        if asset.filetype == Filetype::Archive {
            if name.ends_with(".tar.bz2") || name.ends_with(".tbz") {
                score += 15;
            } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                score += 10;
            } else if name.ends_with(".zip") {
                score += 5;
            }
        }

        if asset.filetype == Filetype::Compressed {
            if name.ends_with(".bz2") {
                score += 10;
            } else if name.ends_with(".gz") {
                score += 5;
            }
        }

        if asset.filetype == Filetype::Binary && Path::new(&name).extension().is_none() {
            score += 10;
        }

        if name.contains("static") {
            score += 5;
        }

        if name.contains("debug") || name.contains("symbols") {
            score -= 20;
        }

        if !name.contains(&package.name.to_lowercase()) {
            score -= 40;
        }

        if asset.size < 100_000 || asset.size > 500_000_000 {
            score -= 20;
        }

        if size_profile.is_some_and(|profile| profile.is_outside_expected_range(asset.size)) {
            score -= 20;
        }

        if !package.match_pattern.is_empty() {
            score += (pattern_match_ratio(&name, &package.match_pattern) * 100.0).round() as i32;
        }

        if !package.exclude_pattern.is_empty() {
            score -= (pattern_match_ratio(&name, &package.exclude_pattern) * 100.0).round() as i32;
        }

        score
    }
}

impl AssetSizeProfile {
    const OUTLIER_FACTOR: u64 = 10;
    const EXPECTED_RANGE_FACTOR: u64 = 2;

    fn from_assets<'a>(assets: impl IntoIterator<Item = &'a Asset>) -> Option<Self> {
        let mut sizes: Vec<u64> = assets
            .into_iter()
            .map(|asset| asset.size)
            .filter(|size| *size > 0)
            .collect();
        if sizes.is_empty() {
            return None;
        }

        sizes.sort_unstable();
        let raw_median = median_sorted(&sizes)?;
        let lower_bound = raw_median.div_ceil(Self::OUTLIER_FACTOR);
        let upper_bound = raw_median.saturating_mul(Self::OUTLIER_FACTOR);
        let trimmed: Vec<u64> = sizes
            .iter()
            .copied()
            .filter(|size| *size >= lower_bound && *size <= upper_bound)
            .collect();

        let median = if trimmed.is_empty() {
            raw_median
        } else {
            median_sorted(&trimmed)?
        };

        Some(Self { median })
    }

    fn is_outside_expected_range(&self, size: u64) -> bool {
        if size == 0 || self.median == 0 {
            return false;
        }

        let lower_bound = self.median.div_ceil(Self::EXPECTED_RANGE_FACTOR);
        let upper_bound = self.median.saturating_mul(Self::EXPECTED_RANGE_FACTOR);
        size < lower_bound || size > upper_bound
    }
}
