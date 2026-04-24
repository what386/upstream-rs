use std::path::Path;

use anyhow::{Result, anyhow};

use crate::models::common::enums::Filetype;
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::utils::platform_info::{ArchitectureInfo, CpuArch, format_arch, format_os};

#[derive(Debug, Clone)]
pub struct AssetCandidate {
    pub asset: Asset,
    pub score: i32,
}

pub struct AssetSelector {
    architecture_info: ArchitectureInfo,
}

impl AssetSelector {
    pub fn new() -> Self {
        Self {
            architecture_info: ArchitectureInfo::new(),
        }
    }

    pub fn find_recommended_asset(&self, release: &Release, package: &Package) -> Result<Asset> {
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

        compatible_assets
            .into_iter()
            .max_by_key(|a| self.score_asset(a, package))
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "No compatible assets found for {} on {}",
                    format_arch(&self.architecture_info.cpu_arch),
                    format_os(&self.architecture_info.os_kind)
                )
            })
    }

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        let target_filetype = if package.filetype == Filetype::Auto {
            Self::resolve_auto_filetype(release)?
        } else {
            package.filetype
        };

        let mut candidates: Vec<AssetCandidate> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| a.filetype == target_filetype)
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: self.score_asset(asset, package),
            })
            .collect();

        candidates.sort_by_key(|b| std::cmp::Reverse(b.score));
        Ok(candidates)
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

    pub fn resolve_auto_filetype(release: &Release) -> Result<Filetype> {
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

    fn is_potentially_compatible(&self, asset: &Asset) -> bool {
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

    fn score_asset(&self, asset: &Asset, package: &Package) -> i32 {
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

        if let Some(pattern) = &package.match_pattern
            && name.contains(pattern)
        {
            score += 100;
        }

        if let Some(antipattern) = &package.exclude_pattern
            && name.contains(antipattern)
        {
            score -= 100;
        }

        score
    }
}

impl Default for AssetSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AssetSelector;
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::provider::{Asset, Release};
    use crate::models::upstream::Package;
    use chrono::Utc;

    fn make_release(assets: Vec<Asset>, prerelease: bool, tag: &str) -> Release {
        Release {
            id: 1,
            tag: tag.to_string(),
            name: tag.to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: prerelease,
            assets,
            version: Version::new(1, 0, 0, prerelease),
            published_at: Utc::now(),
        }
    }

    fn make_package(filetype: Filetype) -> Package {
        Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            filetype,
            Some("static".to_string()),
            Some("debug".to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_auto_filetype_prefers_appimage_then_archives_on_linux() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.AppImage".to_string(),
                    2,
                    "tool.AppImage".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            AssetSelector::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::AppImage
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_auto_filetype_prefers_macapp_on_macos() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.app".to_string(),
                    2,
                    "tool.app".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.dmg".to_string(),
                    3,
                    "tool.dmg".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            AssetSelector::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::MacApp
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_auto_filetype_uses_macdmg_when_no_macapp_exists() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.dmg".to_string(),
                    2,
                    "tool.dmg".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            AssetSelector::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::MacDmg
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_auto_filetype_prefers_winexe_on_windows() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.exe".to_string(),
                    2,
                    "tool.exe".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            AssetSelector::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::WinExe
        );
    }

    #[test]
    fn get_candidate_assets_sorts_by_score_descending() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-debug.tar.gz".to_string(),
                    1,
                    "tool-debug.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-static.tar.bz2".to_string(),
                    2,
                    "tool-static.tar.bz2".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].asset.name, "tool-static.tar.bz2");
        assert!(candidates[0].score > candidates[1].score);
    }

    #[test]
    fn find_recommended_asset_returns_highest_scored_compatible_asset() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-debug.tar.gz".to_string(),
                    1,
                    "tool-debug.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-static.tar.bz2".to_string(),
                    2,
                    "tool-static.tar.bz2".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let best = selector
            .find_recommended_asset(&release, &package)
            .expect("best asset");
        assert_eq!(best.name, "tool-static.tar.bz2");
    }
}
