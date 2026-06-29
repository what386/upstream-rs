use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, anyhow};

use crate::models::common::enums::Filetype;
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::pattern_matcher::{
    GeneratedAssetPatterns, generate_patterns_for_asset, pattern_match_ratio,
};
use crate::utils::math::median_sorted;
use crate::utils::platform::platform_info::{ArchitectureInfo, CpuArch, format_arch, format_os};

#[derive(Debug, Clone)]
pub struct AssetCandidate {
    pub asset: Asset,
    pub score: i32,
}

#[derive(Debug, Clone, Copy)]
struct AssetSizeProfile {
    median: u64,
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
        self.get_candidate_assets(release, package)?
            .into_iter()
            .max_by_key(|candidate| candidate.score)
            .map(|candidate| candidate.asset)
            .ok_or_else(|| {
                anyhow!(
                    "No compatible assets found for {} on {}",
                    format_arch(&self.architecture_info.cpu_arch),
                    format_os(&self.architecture_info.os_kind)
                )
            })
    }

    // 0 B    – 4 KiB   : -80
    // 4 KB   – 16 KiB  : -60
    // 16 KB  – 50 KiB  : -40
    // 50 KB  – 100 KiB : -20
    // 100 KB – 500 MiB : 0
    // > 500 MiB        : -10
    fn absolute_size_score(asset: &Asset) -> i32 {
        match asset.size {
            0..=3_999 => -80,
            4_000..=15_999 => -60,
            16_000..=49_999 => -40,
            50_000..=99_999 => -20,
            500_000_001.. => -10,
            _ => 0,
        }
    }

    #[cfg(unix)]
    fn is_unsupported_package_asset_name(name: &str) -> bool {
        name.ends_with(".deb")
            || name.ends_with(".rpm")
            || name.ends_with(".apk")
            || name.ends_with(".pkg.tar.zst")
            || name.ends_with(".pkg.tar.xz")
            || name.ends_with(".pkg.tar.gz")
            || name.ends_with(".pkg.tar")
            || name.ends_with(".pacman")
            || name.ends_with(".flatpak")
            || name.ends_with(".snap")
    }

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        let target_filetypes = if package.filetype == Filetype::Auto {
            Self::supported_filetypes_for_os()
        } else {
            vec![package.filetype]
        };

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| target_filetypes.contains(&a.filetype))
            .filter(|a| {
                if package.filetype != Filetype::Auto {
                    return true;
                }

                let name = a.name.to_lowercase();

                if Self::is_auxiliary_asset_name(&name) {
                    return false;
                }

                #[cfg(unix)]
                {
                    if Self::is_unsupported_package_asset_name(&name) {
                        return false;
                    }
                }

                true
            })
            .collect();

        if compatible_assets.is_empty() {
            return Err(anyhow!(
                "No compatible assets found for {} on {}",
                format_arch(&self.architecture_info.cpu_arch),
                format_os(&self.architecture_info.os_kind)
            ));
        }

        let size_profiles = Self::size_profiles_by_filetype(&compatible_assets);

        let mut candidates: Vec<AssetCandidate> = compatible_assets
            .into_iter()
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: self.score_asset(asset, package, size_profiles.get(&asset.filetype)),
            })
            .collect();

        candidates.sort_by_key(|b| std::cmp::Reverse(b.score));
        Ok(candidates)
    }

    pub fn get_installable_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Vec<AssetCandidate> {
        self.get_candidate_assets(release, package)
            .unwrap_or_default()
    }

    fn supported_filetypes_for_os() -> Vec<Filetype> {
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
        let priority = Self::supported_filetypes_for_os();

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

    fn filetype_priority_score(filetype: Filetype) -> i32 {
        #[cfg(target_os = "linux")]
        {
            match filetype {
                Filetype::AppImage => 160,
                Filetype::Archive => 60,
                Filetype::Compressed => 40,
                Filetype::Binary => 20,
                _ => -100,
            }
        }

        #[cfg(target_os = "windows")]
        {
            return match filetype {
                Filetype::WinExe => 100,
                Filetype::Archive => 60,
                Filetype::Compressed => 40,
                _ => -100,
            };
        }

        #[cfg(target_os = "macos")]
        {
            return match filetype {
                Filetype::MacApp => 120,
                Filetype::MacDmg => 100,
                Filetype::Archive => 60,
                Filetype::Compressed => 40,
                Filetype::Binary => 20,
                _ => -100,
            };
        }
    }

    fn size_profiles_by_filetype(assets: &[&Asset]) -> HashMap<Filetype, AssetSizeProfile> {
        let mut profiles = HashMap::new();

        for filetype in Self::supported_filetypes_for_os() {
            if let Some(profile) = AssetSizeProfile::from_assets(
                assets
                    .iter()
                    .copied()
                    .filter(|asset| asset.filetype == filetype),
            ) {
                profiles.insert(filetype, profile);
            }
        }

        profiles
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

            return false;
        }

        true
    }

    fn score_asset(
        &self,
        asset: &Asset,
        package: &Package,
        size_profile: Option<&AssetSizeProfile>,
    ) -> i32 {
        let name = asset.name.to_lowercase();
        let package_name = Self::package_identity(package);
        let mut score = 0;

        if package.filetype == Filetype::Auto {
            score += Self::filetype_priority_score(asset.filetype);
        }

        score += Self::primary_name_score(&name, &package_name);
        score += Self::asset_role_score(&name, &package_name);
        score += Self::auxiliary_asset_penalty(&name);

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

        score += Self::absolute_size_score(asset);

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

    fn package_identity(package: &Package) -> String {
        let explicit_name = package.name.trim();
        if !explicit_name.is_empty() {
            return explicit_name.to_lowercase();
        }

        package
            .repo_slug
            .rsplit('/')
            .next()
            .unwrap_or(package.repo_slug.as_str())
            .trim_end_matches(".git")
            .to_lowercase()
    }

    fn primary_name_score(name: &str, package_name: &str) -> i32 {
        if package_name.is_empty() {
            return 0;
        }

        let stem = Self::strip_known_asset_suffixes(name);

        if stem == package_name {
            return 80;
        }

        if Self::starts_with_primary_target(&stem, package_name) {
            return 50;
        }

        if stem.starts_with(&format!("{package_name}-"))
            || stem.starts_with(&format!("{package_name}_"))
        {
            return 10;
        }

        if Self::contains_name_token(&stem, package_name) {
            return 0;
        }

        -60
    }

    fn starts_with_primary_target(stem: &str, package_name: &str) -> bool {
        const PRIMARY_TARGET_PREFIXES: &[&str] = &[
            "x86_64",
            "amd64",
            "x64",
            "aarch64",
            "arm64",
            "arm",
            "x86",
            "i686",
            "linux",
            "darwin",
            "macos",
            "windows",
            "win32",
            "win64",
            "musl",
            "gnu",
            "manylinux",
        ];

        PRIMARY_TARGET_PREFIXES.iter().any(|target| {
            stem.starts_with(&format!("{package_name}-{target}"))
                || stem.starts_with(&format!("{package_name}_{target}"))
        })
    }

    fn contains_name_token(value: &str, package_name: &str) -> bool {
        value
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == package_name)
    }

    fn asset_role_score(name: &str, package_name: &str) -> i32 {
        let mut score = 0;
        let tokens = Self::asset_name_tokens(name);

        let has = |token: &str| tokens.contains(&token);

        if has("cli") {
            score += 20;
        }

        if has("bin") || has("binary") {
            score += 15;
        }

        if has("standalone") || has("portable") || has("bundle") {
            score += 10;
        }

        if has("server") {
            score -= 25;
        }

        if has("proxy") {
            score -= 25;
        }

        if has("sdk") {
            score -= 25;
        }

        if has("npm") {
            score -= 25;
        }

        if has("zsh") || has("bash") || has("fish") || has("completion") || has("completions") {
            score -= 25;
        }

        if has("package") {
            score -= 10;
        }

        if has("app") && !package_name.contains("app") {
            score -= 10;
        }

        score
    }

    fn auxiliary_asset_penalty(name: &str) -> i32 {
        let mut penalty = 0;

        if name.contains("symbols") || name.contains("debug") || name.contains("pdb") {
            penalty -= 80;
        }

        if Self::is_auxiliary_asset_name(name) {
            penalty -= 100;
        }

        if Self::is_installer_script_name(name) {
            penalty -= 80;
        }

        penalty
    }

    fn is_auxiliary_asset_name(name: &str) -> bool {
        name.ends_with(".sig")
            || name.ends_with(".asc")
            || name.ends_with(".sigstore")
            || name.ends_with(".sha256")
            || name.ends_with(".sha256sum")
            || name.ends_with(".sha256sums")
            || name.ends_with("_sha256sum")
            || name.ends_with("_sha256sums")
            || name.ends_with(".checksums")
            || name.ends_with(".checksum")
            || name.ends_with(".json")
            || name.ends_with(".spdx")
            || name.ends_with(".sbom")
            || name.ends_with(".txt")
    }

    fn is_installer_script_name(name: &str) -> bool {
        matches!(
            name,
            "install.sh"
                | "install.ps1"
                | "install.bat"
                | "install.cmd"
                | "installer.sh"
                | "installer.ps1"
                | "setup.sh"
                | "setup.ps1"
        )
    }

    fn asset_name_tokens(name: &str) -> Vec<&str> {
        name.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn strip_known_asset_suffixes(name: &str) -> String {
        const SUFFIXES: &[&str] = &[
            ".tar.bz2",
            ".tar.gz",
            ".tar.xz",
            ".tar.zst",
            ".appimage",
            ".dmg",
            ".pkg",
            ".msi",
            ".exe",
            ".tgz",
            ".tbz",
            ".txz",
            ".zip",
            ".whl",
            ".gz",
            ".bz2",
            ".xz",
            ".zst",
        ];

        let mut stem = name;

        while let Some(suffix) = SUFFIXES
            .iter()
            .find(|suffix| stem.ends_with(**suffix))
            .copied()
        {
            stem = &stem[..stem.len() - suffix.len()];
        }

        stem.to_string()
    }

    pub fn generate_patterns_for_asset(
        &self,
        selected: &Asset,
        release_assets: &[Asset],
        package_name: &str,
    ) -> GeneratedAssetPatterns {
        generate_patterns_for_asset(selected, release_assets, package_name)
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
    fn get_candidate_assets_penalizes_sizes_far_from_trimmed_median() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-normal.tar.gz".to_string(),
                    1,
                    "tool-normal.tar.gz".to_string(),
                    10_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-peer.tar.gz".to_string(),
                    2,
                    "tool-peer.tar.gz".to_string(),
                    11_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-large.tar.gz".to_string(),
                    3,
                    "tool-large.tar.gz".to_string(),
                    25_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-huge.tar.gz".to_string(),
                    4,
                    "tool-huge.tar.gz".to_string(),
                    600_000_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        let score_for = |name: &str| {
            candidates
                .iter()
                .find(|candidate| candidate.asset.name == name)
                .map(|candidate| candidate.score)
                .expect("candidate score")
        };

        assert_eq!(
            score_for("tool-large.tar.gz"),
            score_for("tool-normal.tar.gz") - 20
        );

        assert_eq!(
            score_for("tool-huge.tar.gz"),
            score_for("tool-normal.tar.gz") - 30
        );
    }

    #[test]
    fn get_installable_candidate_assets_keeps_all_supported_auto_filetypes() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
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
                    "https://example.invalid/tool.gz".to_string(),
                    2,
                    "tool.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.sha256".to_string(),
                    3,
                    "tool.sha256".to_string(),
                    1_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector.get_installable_candidate_assets(&release, &package);

        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().any(|c| c.asset.name == "tool.tar.gz"));
        assert!(candidates.iter().any(|c| c.asset.name == "tool.gz"));
        assert!(!candidates.iter().any(|c| c.asset.name == "tool.sha256"));
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

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_scores_across_supported_filetypes_instead_of_hard_selecting_appimage() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/not-the-tool-debug.AppImage".to_string(),
                    1,
                    "not-the-tool-debug.AppImage".to_string(),
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

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_still_prefers_appimage_when_candidates_are_otherwise_good() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
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

        let best = selector
            .find_recommended_asset(&release, &package)
            .expect("best asset");

        assert_eq!(best.name, "tool.AppImage");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_candidate_assets_include_all_supported_filetypes_sorted_by_score() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
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
                    "https://example.invalid/tool.gz".to_string(),
                    2,
                    "tool.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.AppImage".to_string(),
                    3,
                    "tool.AppImage".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.sha256".to_string(),
                    4,
                    "tool.sha256".to_string(),
                    1_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        assert!(candidates.iter().any(|c| c.asset.name == "tool.AppImage"));
        assert!(candidates.iter().any(|c| c.asset.name == "tool.tar.gz"));
        assert!(candidates.iter().any(|c| c.asset.name == "tool.gz"));
        assert!(!candidates.iter().any(|c| c.asset.name == "tool.sha256"));

        assert_eq!(candidates[0].asset.name, "tool.AppImage");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn primary_package_asset_beats_subcomponent_assets() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            String::new(),
            "owner/codex".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/codex-app-server-package-x86_64-unknown-linux-musl.tar.gz"
                        .to_string(),
                    1,
                    "codex-app-server-package-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    86_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    2,
                    "codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    99_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/codex-responses-api-proxy-x86_64-unknown-linux-musl.tar.gz"
                        .to_string(),
                    3,
                    "codex-responses-api-proxy-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    4_000_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let best = selector
            .find_recommended_asset(&release, &package)
            .expect("best asset");

        assert_eq!(best.name, "codex-x86_64-unknown-linux-musl.tar.gz");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_filters_auxiliary_assets_from_installable_candidates() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
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
                    "https://example.invalid/tool.sigstore".to_string(),
                    2,
                    "tool.sigstore".to_string(),
                    8_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool_SHA256SUMS".to_string(),
                    3,
                    "tool_SHA256SUMS".to_string(),
                    1_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/config-schema.json".to_string(),
                    4,
                    "config-schema.json".to_string(),
                    155_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].asset.name, "tool.tar.gz");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_filters_unsupported_package_assets() {
        let selector = AssetSelector::new();
        let package = make_package(Filetype::Auto);
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
                    "https://example.invalid/tool.deb".to_string(),
                    2,
                    "tool.deb".to_string(),
                    2_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.rpm".to_string(),
                    3,
                    "tool.rpm".to_string(),
                    2_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.pkg.tar.zst".to_string(),
                    4,
                    "tool.pkg.tar.zst".to_string(),
                    2_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.snap".to_string(),
                    5,
                    "tool.snap".to_string(),
                    2_000_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].asset.name, "tool.tar.gz");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn symbols_assets_are_heavily_demoted() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            String::new(),
            "owner/codex".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/codex-symbols-x86_64-unknown-linux-musl.tar.gz"
                        .to_string(),
                    1,
                    "codex-symbols-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    197_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    2,
                    "codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    99_000_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        assert_eq!(
            candidates[0].asset.name,
            "codex-x86_64-unknown-linux-musl.tar.gz"
        );
        assert!(candidates[0].score > candidates[1].score);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn repo_slug_basename_is_used_when_package_name_is_empty() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            String::new(),
            "openai/codex".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/argument-comment-lint-x86_64-unknown-linux-gnu.tar.gz"
                        .to_string(),
                    1,
                    "argument-comment-lint-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    3_000_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    2,
                    "codex-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    99_000_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let best = selector
            .find_recommended_asset(&release, &package)
            .expect("best asset");

        assert_eq!(best.name, "codex-x86_64-unknown-linux-musl.tar.gz");
    }
}
