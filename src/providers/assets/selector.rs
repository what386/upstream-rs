use anyhow::{Result, anyhow};

use crate::models::common::enums::Filetype;
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::assets::filenames::parser::is_unsupported_artifact_name;
use crate::providers::assets::scoring::{
    is_auxiliary_asset_name, is_potentially_compatible, score_asset, size_profiles_by_filetype,
    supported_filetypes_for_os,
};
use crate::providers::pattern_matcher::{GeneratedAssetPatterns, generate_patterns_for_asset};
use crate::utils::platform::platform_info::{ArchitectureInfo, format_arch, format_os};

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

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        let target_filetypes = if package.filetype == Filetype::Auto {
            supported_filetypes_for_os()
        } else {
            vec![package.filetype]
        };

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|asset| !is_unsupported_artifact_name(&asset.name))
            .filter(|asset| is_potentially_compatible(asset, &self.architecture_info))
            .filter(|asset| target_filetypes.contains(&asset.filetype))
            .filter(|asset| {
                package.filetype != Filetype::Auto
                    || !is_auxiliary_asset_name(&asset.name.to_lowercase())
            })
            .collect();

        if compatible_assets.is_empty() {
            return Err(anyhow!(
                "No compatible assets found for {} on {}",
                format_arch(&self.architecture_info.cpu_arch),
                format_os(&self.architecture_info.os_kind)
            ));
        }

        let size_profiles = size_profiles_by_filetype(&compatible_assets);
        let mut candidates: Vec<AssetCandidate> = compatible_assets
            .into_iter()
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: score_asset(
                    asset,
                    package,
                    &self.architecture_info,
                    size_profiles.get(&asset.filetype),
                ),
            })
            .collect();

        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
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

    pub fn resolve_auto_filetype(release: &Release) -> Result<Filetype> {
        supported_filetypes_for_os()
            .into_iter()
            .find(|filetype| {
                release
                    .assets
                    .iter()
                    .any(|asset| asset.filetype == *filetype)
            })
            .ok_or_else(|| anyhow!("No compatible filetype found in release assets"))
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
    use crate::providers::assets::scoring;
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

    #[test]
    fn forge_asset_identity_uses_repository_name_instead_of_local_alias() {
        let package = Package::with_defaults(
            "ripgrep".to_string(),
            "BurntSushi/ripgrep".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        assert_eq!(scoring::package_identity(&package), "ripgrep");
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
    fn auto_prefers_explicit_matching_os_over_generic_archive() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            "yt-dlp".to_string(),
            "owner/yt-dlp".to_string(),
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
                    "https://example.invalid/yt-dlp.tar.gz".to_string(),
                    1,
                    "yt-dlp.tar.gz".to_string(),
                    5_740_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/yt-dlp_linux.zip".to_string(),
                    2,
                    "yt-dlp_linux.zip".to_string(),
                    38_130_000,
                    Utc::now(),
                ),
            ],
            false,
            "2026.07.04",
        );

        let candidates = selector
            .get_candidate_assets(&release, &package)
            .expect("candidates");

        assert_eq!(candidates[0].asset.name, "yt-dlp_linux.zip");
        assert_eq!(candidates[0].score, 195);
        assert_eq!(
            candidates
                .iter()
                .find(|candidate| candidate.asset.name == "yt-dlp.tar.gz")
                .expect("generic archive")
                .score,
            130
        );
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
