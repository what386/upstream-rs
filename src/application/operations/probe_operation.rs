use anyhow::{Result, anyhow};
use std::collections::HashMap;

use crate::{
    models::{
        common::enums::{Channel, Filetype, Provider},
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::{
        asset_selector::{AssetCandidate, AssetSelector},
        discovery::DiscoveryRequest,
        provider_manager::ProviderManager,
    },
    services::packaging::disk_impact::{
        DiskImpact, asset_size_estimate, install_impact_from_download,
    },
};

pub struct ProbeRequest {
    pub input: String,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub limit: u32,
}

pub struct ProbeResult {
    pub input: String,
    pub repo_slug: String,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub notes: Vec<String>,
    pub releases: Vec<Release>,
    pub probe_package: Package,
    pub rows: Vec<ProbeRow>,
    pub choices: Vec<ProbeAssetChoice>,
}

pub struct ProbeInstallSelection {
    pub package: Package,
    pub release: Release,
    pub asset: Asset,
    pub disk_impact: DiskImpact,
}

pub struct ProbeOperation<'a> {
    provider_manager: &'a ProviderManager,
}

impl<'a> ProbeOperation<'a> {
    pub fn new(provider_manager: &'a ProviderManager) -> Self {
        Self { provider_manager }
    }

    pub async fn probe(&self, request: ProbeRequest) -> Result<ProbeResult> {
        let mut notes = Vec::new();
        let (repo_slug, provider, base_url, mut releases) = if let Some(provider) = request.provider
        {
            notes.push(format!("Probing '{}' via {}", request.input, provider));

            let releases = self
                .provider_manager
                .get_releases(
                    &request.input,
                    &provider,
                    Some(request.limit),
                    Some(request.limit),
                    request.base_url.as_deref(),
                )
                .await?;
            (
                request.input.clone(),
                provider,
                request.base_url.clone(),
                releases,
            )
        } else {
            let discovery = self
                .provider_manager
                .discover_source(DiscoveryRequest {
                    source: request.input.clone(),
                    channel: request.channel.clone(),
                    package_name: String::new(),
                    filetype: Filetype::Auto,
                    match_pattern: None,
                    exclude_pattern: None,
                    base_url_override: request.base_url.clone(),
                    limit: request.limit,
                })
                .await?;

            notes.push(format!(
                "Probing '{}' as '{}' via {}",
                request.input, discovery.source.repo_slug, discovery.source.provider
            ));

            (
                discovery.source.repo_slug,
                discovery.source.provider,
                discovery.source.base_url,
                discovery.releases,
            )
        };

        releases = filter_by_channel(releases, &request.channel);
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        let probe_package = Package::with_defaults(
            String::new(),
            repo_slug.clone(),
            Filetype::Auto,
            None,
            None,
            request.channel.clone(),
            provider.clone(),
            base_url.clone(),
        );
        let rows = build_probe_rows(&releases, self.provider_manager, &probe_package);
        let choices = build_probe_asset_choices(&releases, self.provider_manager, &probe_package);

        Ok(ProbeResult {
            input: request.input,
            repo_slug,
            provider,
            base_url,
            channel: request.channel,
            notes,
            releases,
            probe_package,
            rows,
            choices,
        })
    }

    pub fn prepare_install_selection(
        &self,
        result: &ProbeResult,
        selected_index: usize,
        install_name: String,
    ) -> Result<ProbeInstallSelection> {
        let selected_choice = result
            .choices
            .get(selected_index)
            .ok_or_else(|| anyhow!("Selected asset no longer exists"))?;
        let selected_release = result
            .releases
            .get(selected_choice.release_index)
            .cloned()
            .ok_or_else(|| anyhow!("Selected release no longer exists"))?;
        let selected_asset = selected_choice.asset.clone();
        let generated = AssetSelector::new().generate_patterns_for_asset(
            &selected_asset,
            &selected_release.assets,
            &install_name,
        );

        let package = Package::with_defaults(
            install_name,
            result.repo_slug.clone(),
            selected_asset.filetype,
            Some(generated.match_pattern.to_string()),
            Some(generated.exclude_pattern.to_string()),
            result.channel.clone(),
            result.provider.clone(),
            result.base_url.clone(),
        );
        let disk_impact = install_impact_from_download(asset_size_estimate(selected_asset.size));

        Ok(ProbeInstallSelection {
            package,
            release: selected_release,
            asset: selected_asset,
            disk_impact,
        })
    }
}

pub fn build_probe_asset_choices(
    releases: &[Release],
    provider_manager: &ProviderManager,
    probe_package: &Package,
) -> Vec<ProbeAssetChoice> {
    let mut choices = Vec::new();

    for (release_index, release) in releases.iter().enumerate() {
        let score_by_asset_id: HashMap<u64, i32> = provider_manager
            .get_candidate_assets(release, probe_package)
            .map(|candidates| {
                candidates
                    .into_iter()
                    .map(|candidate| (candidate.asset.id, candidate.score))
                    .collect()
            })
            .unwrap_or_default();

        for asset in &release.assets {
            choices.push(ProbeAssetChoice {
                release_index,
                release_tag: release.tag.clone(),
                release_state: release_state(release.is_draft, release.is_prerelease),
                asset: asset.clone(),
                score: score_by_asset_id.get(&asset.id).copied(),
            });
        }
    }

    choices
}

pub fn build_probe_rows(
    releases: &[Release],
    provider_manager: &ProviderManager,
    probe_package: &Package,
) -> Vec<ProbeRow> {
    releases
        .iter()
        .enumerate()
        .map(|(idx, release)| {
            let candidates_result = provider_manager.get_candidate_assets(release, probe_package);

            let (top_candidate, candidates, candidate_error) = match candidates_result {
                Ok(list) => {
                    let top = list
                        .first()
                        .map(|c| format!("{} ({})", c.asset.name, c.score))
                        .unwrap_or_else(|| "-".to_string());
                    (top, Some(list), None)
                }
                Err(err) => ("n/a".to_string(), None, Some(err.to_string())),
            };

            ProbeRow {
                row_id: format!("R{:02}", idx + 1),
                state: release_state(release.is_draft, release.is_prerelease),
                tag: release.tag.clone(),
                version: release.version.to_string(),
                published: release.published_at.format("%Y-%m-%d %H:%M").to_string(),
                assets_count: release.assets.len(),
                top_candidate,
                candidates,
                candidate_error,
            }
        })
        .collect()
}

pub fn filter_by_channel(mut releases: Vec<Release>, channel: &Channel) -> Vec<Release> {
    match channel {
        Channel::Stable => {
            releases.retain(|r| !r.is_prerelease && !ProviderManager::is_nightly_release(&r.tag))
        }
        Channel::Preview => releases.retain(ProviderManager::is_preview_release),
        Channel::Nightly => releases.retain(|r| ProviderManager::is_nightly_release(&r.tag)),
    }
    releases
}

pub fn release_state(is_draft: bool, is_prerelease: bool) -> ReleaseState {
    match (is_draft, is_prerelease) {
        (false, false) => ReleaseState::Release,
        (false, true) => ReleaseState::Preview,
        (true, false) => ReleaseState::Draft,
        (true, true) => ReleaseState::DraftPre,
    }
}

#[derive(Debug, Clone)]
pub struct ProbeAssetChoice {
    pub release_index: usize,
    pub release_tag: String,
    pub release_state: ReleaseState,
    pub asset: Asset,
    pub score: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ProbeRow {
    pub row_id: String,
    pub state: ReleaseState,
    pub tag: String,
    pub version: String,
    pub published: String,
    pub assets_count: usize,
    pub top_candidate: String,
    pub candidates: Option<Vec<AssetCandidate>>,
    pub candidate_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ReleaseState {
    Release,
    Preview,
    Draft,
    DraftPre,
}

impl ReleaseState {
    pub fn label(&self) -> &'static str {
        match self {
            ReleaseState::Release => "release",
            ReleaseState::Preview => "preview",
            ReleaseState::Draft => "draft",
            ReleaseState::DraftPre => "draft+pre",
        }
    }
}
