use anyhow::Result;

use crate::models::common::enums::Filetype;
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::heuristic_asset_selector::HeuristicAssetSelector;
use crate::providers::neural_asset_selector::NeuralAssetSelector;
use crate::providers::pattern_matcher::{
    GeneratedAssetPatterns, generate_patterns_for_asset, pattern_match_ratio,
};
use crate::utils::platform::platform_info::ArchitectureInfo;

#[derive(Debug, Clone)]
pub struct AssetCandidate {
    pub asset: Asset,
    pub score: i32,
}

pub struct AssetSelector {
    heuristic_selector: HeuristicAssetSelector,
    neural_selector: Option<NeuralAssetSelector>,
}

impl AssetSelector {
    pub fn new() -> Self {
        let architecture_info = ArchitectureInfo::new();
        let heuristic_selector = HeuristicAssetSelector::new(architecture_info.clone());
        let neural_selector = NeuralAssetSelector::with_architecture(architecture_info).ok();
        Self {
            heuristic_selector,
            neural_selector,
        }
    }

    pub fn find_recommended_asset(&self, release: &Release, package: &Package) -> Result<Asset> {
        if let Some(candidates) = self.neural_candidate_assets(release, package)
            && let Some(candidate) = candidates.into_iter().next()
        {
            return Ok(candidate.asset);
        }

        self.heuristic_selector
            .find_recommended_asset(release, package)
    }

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        if let Some(candidates) = self.neural_candidate_assets(release, package) {
            return Ok(candidates);
        }

        self.heuristic_selector
            .get_candidate_assets(release, package)
    }

    pub fn get_installable_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Vec<AssetCandidate> {
        if let Some(candidates) = self.neural_candidate_assets(release, package) {
            return candidates;
        }

        self.heuristic_selector
            .get_installable_candidate_assets(release, package)
    }

    fn neural_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Option<Vec<AssetCandidate>> {
        let neural_selector = self.neural_selector.as_ref()?;
        let target_filetypes = self.heuristic_selector.target_filetypes(package.filetype);
        let compatible_assets: Vec<Asset> = release
            .assets
            .iter()
            .filter(|asset| target_filetypes.contains(&asset.filetype))
            .cloned()
            .collect();
        if compatible_assets.is_empty() {
            return None;
        }

        let mut filtered_release = release.clone();
        filtered_release.assets = compatible_assets;
        let prediction = neural_selector
            .predict(&filtered_release, package, filtered_release.assets.len())
            .ok()?;
        let mut candidates: Vec<AssetCandidate> = prediction
            .alternatives
            .into_iter()
            .map(|alternative| {
                let asset = filtered_release.assets[alternative.asset_index].clone();
                let score = neural_score_to_i32(alternative.score)
                    .saturating_add(neural_pattern_adjustment(&asset.name, package));
                AssetCandidate { asset, score }
            })
            .collect();
        candidates = apply_pattern_filters(candidates, package);
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
        Some(candidates)
    }

    pub fn resolve_auto_filetype(release: &Release) -> Result<Filetype> {
        HeuristicAssetSelector::resolve_auto_filetype(release)
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

fn neural_score_to_i32(score: f64) -> i32 {
    let scaled = (score * 10_000.0).round();
    if scaled < i32::MIN as f64 {
        i32::MIN
    } else if scaled > i32::MAX as f64 {
        i32::MAX
    } else {
        scaled as i32
    }
}
fn neural_pattern_adjustment(asset_name: &str, package: &Package) -> i32 {
    const PATTERN_SCORE_WEIGHT: f64 = 1_000_000.0;

    let name = asset_name.to_lowercase();
    let mut adjustment = 0.0;
    if !package.match_pattern.is_empty() {
        adjustment += pattern_match_ratio(&name, &package.match_pattern) * PATTERN_SCORE_WEIGHT;
    }
    if !package.exclude_pattern.is_empty() {
        adjustment -= pattern_match_ratio(&name, &package.exclude_pattern) * PATTERN_SCORE_WEIGHT;
    }
    adjustment.round() as i32
}
fn apply_pattern_filters(
    mut candidates: Vec<AssetCandidate>,
    package: &Package,
) -> Vec<AssetCandidate> {
    if !package.match_pattern.is_empty() {
        let matching: Vec<AssetCandidate> = candidates
            .iter()
            .filter(|candidate| {
                pattern_match_ratio(&candidate.asset.name, &package.match_pattern) > 0.0
            })
            .cloned()
            .collect();
        if !matching.is_empty() {
            candidates = matching;
        }
    }

    if !package.exclude_pattern.is_empty() {
        let non_excluded: Vec<AssetCandidate> = candidates
            .iter()
            .filter(|candidate| {
                pattern_match_ratio(&candidate.asset.name, &package.exclude_pattern) <= 0.0
            })
            .cloned()
            .collect();
        if !non_excluded.is_empty() {
            candidates = non_excluded;
        }
    }

    candidates
}

#[cfg(test)]
impl AssetSelector {
    fn new_without_neural() -> Self {
        let architecture_info = ArchitectureInfo::new();
        Self {
            heuristic_selector: HeuristicAssetSelector::new(architecture_info),
            neural_selector: None,
        }
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
    fn make_neural_package(filetype: Filetype) -> Package {
        Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            filetype,
            None,
            None,
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
        let selector = AssetSelector::new_without_neural();
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
        let selector = AssetSelector::new_without_neural();
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
            score_for("tool-normal.tar.gz") - 40
        );
    }

    #[test]
    fn neural_candidates_filter_filetype_but_not_parsed_platform_or_arch() {
        let selector = AssetSelector::new();
        let package = make_neural_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-x86_64-pc-windows-msvc.zip".to_string(),
                    1,
                    "tool-x86_64-pc-windows-msvc.zip".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-aarch64-apple-darwin.tar.gz".to_string(),
                    2,
                    "tool-aarch64-apple-darwin.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    3,
                    "tool-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.exe".to_string(),
                    4,
                    "tool.exe".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = selector.get_installable_candidate_assets(&release, &package);

        assert_eq!(candidates.len(), 3);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.asset.name == "tool-x86_64-pc-windows-msvc.zip")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.asset.name == "tool-aarch64-apple-darwin.tar.gz")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.asset.name == "tool-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.asset.name == "tool.exe")
        );
    }
    #[test]
    fn neural_candidates_apply_match_patterns_after_model_scoring() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            Some("forcewinner".to_string()),
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    1,
                    "tool-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-forcewinner.tar.gz".to_string(),
                    2,
                    "tool-forcewinner.tar.gz".to_string(),
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

        assert_eq!(candidates[0].asset.name, "tool-forcewinner.tar.gz");
    }
    #[test]
    fn neural_candidates_apply_exclude_patterns_after_model_scoring() {
        let selector = AssetSelector::new();
        let package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            None,
            Some("forceblock".to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        );
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-forceblock.tar.gz".to_string(),
                    1,
                    "tool-forceblock.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-keep.tar.gz".to_string(),
                    2,
                    "tool-keep.tar.gz".to_string(),
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

        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.asset.name != "tool-forceblock.tar.gz")
        );
    }
    #[test]
    fn get_installable_candidate_assets_keeps_all_supported_auto_filetypes() {
        let selector = AssetSelector::new_without_neural();
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
        let selector = AssetSelector::new_without_neural();
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
