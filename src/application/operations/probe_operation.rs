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
        discovery::infer_source,
        provider_manager::ProviderManager,
    },
    services::packaging::disk_impact::{
        DiskImpact, asset_size_estimate, install_impact_from_download,
    },
};

const DEFAULT_PROBE_RELEASE_LIMIT: u32 = 10;

pub struct ProbeRequest {
    pub input: String,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub limit: u32,
    pub release_selector: ProbeReleaseSelector,
    pub include_incompatible: bool,
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
        let (repo_slug, provider, base_url) = if let Some(provider) = request.provider.clone() {
            notes.push(format!("Probing '{}' via {}", request.input, provider));
            (request.input.clone(), provider, request.base_url.clone())
        } else {
            let mut discovery = infer_source(&request.input)?;
            if let Some(base_url) = request.base_url.as_deref() {
                discovery.base_url = Some(base_url.to_string());
            }

            notes.push(format!(
                "Probing '{}' as '{}' via {}",
                request.input, discovery.repo_slug, discovery.provider
            ));

            (discovery.repo_slug, discovery.provider, discovery.base_url)
        };

        let mut releases = self
            .fetch_releases(&repo_slug, &provider, base_url.as_deref(), &request)
            .await?;
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
        let choices = build_probe_asset_choices(
            &releases,
            self.provider_manager,
            &probe_package,
            request.include_incompatible,
        );

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

    async fn fetch_releases(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        request: &ProbeRequest,
    ) -> Result<Vec<Release>> {
        match &request.release_selector {
            ProbeReleaseSelector::Latest => {
                let release = self
                    .provider_manager
                    .get_latest_release(repo_slug, provider, &request.channel, base_url)
                    .await?;
                Ok(vec![release])
            }
            ProbeReleaseSelector::All => {
                let releases = self
                    .provider_manager
                    .get_releases(
                        repo_slug,
                        provider,
                        Some(request.limit),
                        Some(request.limit),
                        base_url,
                    )
                    .await?;
                Ok(filter_by_channel(releases, &request.channel))
            }
            ProbeReleaseSelector::Tag(tag) => {
                let release = self
                    .provider_manager
                    .get_release_by_tag(repo_slug, tag, provider, base_url)
                    .await
                    .map_err(|err| anyhow!("Failed to fetch release tag '{}': {}", tag, err))?;
                Ok(vec![release])
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeReleaseSelector {
    Latest,
    All,
    Tag(String),
}

impl ProbeReleaseSelector {
    pub fn from_cli_options(tag: Option<String>, limit: Option<u32>) -> Result<(Self, u32)> {
        let Some(tag) = tag else {
            return Ok((
                if limit.is_some() {
                    Self::All
                } else {
                    Self::Latest
                },
                limit.unwrap_or(DEFAULT_PROBE_RELEASE_LIMIT),
            ));
        };

        let selector = Self::from_cli_value(&tag)?;
        if limit.is_some() && !matches!(selector, Self::All) {
            return Err(anyhow!(
                "--limit only applies when probing all releases; use --tag all --limit {}",
                limit.unwrap_or(DEFAULT_PROBE_RELEASE_LIMIT)
            ));
        }

        Ok((selector, limit.unwrap_or(DEFAULT_PROBE_RELEASE_LIMIT)))
    }

    fn from_cli_value(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Probe release selector cannot be empty"));
        }

        if trimmed.eq_ignore_ascii_case("latest") {
            return Ok(Self::Latest);
        }

        if trimmed.eq_ignore_ascii_case("all") {
            return Ok(Self::All);
        }

        Ok(Self::Tag(trimmed.to_string()))
    }
}

pub fn build_probe_asset_choices(
    releases: &[Release],
    provider_manager: &ProviderManager,
    probe_package: &Package,
    include_incompatible: bool,
) -> Vec<ProbeAssetChoice> {
    let mut choices = Vec::new();

    for (release_index, release) in releases.iter().enumerate() {
        let candidates = provider_manager
            .get_candidate_assets(release, probe_package)
            .unwrap_or_default();

        if include_incompatible {
            let score_by_asset_id: HashMap<u64, i32> = candidates
                .into_iter()
                .map(|candidate| (candidate.asset.id, candidate.score))
                .collect();

            for asset in &release.assets {
                choices.push(ProbeAssetChoice {
                    release_index,
                    release_tag: release.tag.clone(),
                    release_state: release_state(release.is_draft, release.is_prerelease),
                    asset: asset.clone(),
                    score: score_by_asset_id.get(&asset.id).copied(),
                });
            }
        } else {
            choices.extend(candidates.into_iter().map(|candidate| ProbeAssetChoice {
                release_index,
                release_tag: release.tag.clone(),
                release_state: release_state(release.is_draft, release.is_prerelease),
                asset: candidate.asset,
                score: Some(candidate.score),
            }));
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
