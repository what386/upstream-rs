use anyhow::Result;
use console::style;
use indicatif::HumanBytes;

use crate::{
    models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    },
    providers::provider_manager::{AssetCandidate, ProviderManager},
    services::storage::config_storage::ConfigStorage,
    utils::static_paths::UpstreamPaths,
};

pub async fn run(
    repo_slug: String,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    limit: u32,
    verbose: bool,
) -> Result<()> {
    let effective_provider = provider.unwrap_or_else(|| infer_provider(&repo_slug));
    let paths = UpstreamPaths::new();
    let config = ConfigStorage::new(&paths.config.config_file)?;

    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();
    let gitea_token = config.get_config().gitea.api_token.as_deref();

    let provider_manager =
        ProviderManager::new(github_token, gitlab_token, gitea_token, base_url.as_deref())?;

    println!(
        "{}",
        style(format!("Probing '{}' via {} ...", repo_slug, effective_provider)).cyan()
    );

    let mut releases = provider_manager
        .get_releases(&repo_slug, &effective_provider, Some(limit), Some(limit))
        .await?;

    releases = filter_by_channel(releases, &channel);
    releases.sort_by(|a, b| b.version.cmp(&a.version));

    if releases.is_empty() {
        println!("No releases found for channel '{}'.", channel);
        return Ok(());
    }

    let probe_package = Package::with_defaults(
        String::new(),
        repo_slug.clone(),
        Filetype::Auto,
        None,
        None,
        channel.clone(),
        effective_provider.clone(),
        base_url.clone(),
    );

    let rows = build_probe_rows(&releases, &provider_manager, &probe_package);
    let widths = ProbeColumnWidths::from_rows(&rows);

    let header = format!(
        "{:<id$} {:<state$} {:<tag$} {:<ver$} {:<pubd$} {:<assets$} {}",
        "ID",
        "State",
        "Tag",
        "Version",
        "Published",
        "Assets",
        "Top Candidate",
        id = widths.id,
        state = widths.state,
        tag = widths.tag,
        ver = widths.version,
        pubd = widths.published,
        assets = widths.assets
    );
    println!("{}", style(header).bold());
    println!("{}", "-".repeat(widths.table_width()));

    for row in &rows {
        println!(
            "{:<id$} {} {:<tag$} {:<ver$} {:<pubd$} {:<assets$} {}",
            row.row_id,
            format_state_cell(&row.state, widths.state),
            truncate(&row.tag, widths.tag),
            truncate(&row.version, widths.version),
            row.published,
            row.assets_count,
            truncate(&row.top_candidate, widths.top_candidate),
            id = widths.id,
            tag = widths.tag,
            ver = widths.version,
            pubd = widths.published,
            assets = widths.assets
        );

        if verbose {
            render_candidates(row);
        }
    }

    Ok(())
}

fn render_candidates(row: &ProbeRow) {
    let Some(candidates) = row.candidates.as_ref() else {
        println!(
            "     candidates: failed ({})",
            truncate(row.candidate_error.as_deref().unwrap_or("unknown"), 48)
        );
        return;
    };

    if candidates.is_empty() {
        println!("     candidates: none");
        return;
    }

    println!("     candidates:");
    for (rank, candidate) in candidates.iter().take(6).enumerate() {
        let asset = &candidate.asset;
        println!(
            "       #{} {:<44} {:>11} {:<10} score={}",
            rank + 1,
            truncate(&asset.name, 46),
            HumanBytes(asset.size),
            format!("{:?}", asset.filetype),
            candidate.score
        );
    }
    if candidates.len() > 6 {
        println!("       ... and {} more", candidates.len() - 6);
    }
}

fn build_probe_rows(
    releases: &[crate::models::provider::Release],
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

fn release_state(is_draft: bool, is_prerelease: bool) -> ReleaseState {
    match (is_draft, is_prerelease) {
        (false, false) => ReleaseState::Release,
        (false, true) => ReleaseState::Preview,
        (true, false) => ReleaseState::Draft,
        (true, true) => ReleaseState::DraftPre,
    }
}

fn format_state_cell(state: &ReleaseState, width: usize) -> String {
    let padded = format!("{:<width$}", state.label(), width = width);
    match state {
        ReleaseState::Release => style(padded).green().to_string(),
        ReleaseState::Preview => style(padded).yellow().to_string(),
        ReleaseState::Draft => style(padded).blue().to_string(),
        ReleaseState::DraftPre => style(padded).magenta().to_string(),
    }
}

fn infer_provider(repo_or_url: &str) -> Provider {
    let value = repo_or_url.trim().to_lowercase();
    if value.starts_with("http://") || value.starts_with("https://") {
        Provider::WebScraper
    } else {
        Provider::Github
    }
}

fn filter_by_channel(
    mut releases: Vec<crate::models::provider::Release>,
    channel: &Channel,
) -> Vec<crate::models::provider::Release> {
    match channel {
        Channel::Stable => {
            releases.retain(|r| !r.is_prerelease && !ProviderManager::is_nightly_release(&r.tag))
        }
        Channel::Preview => releases.retain(ProviderManager::is_preview_release),
        Channel::Nightly => releases.retain(|r| ProviderManager::is_nightly_release(&r.tag)),
    }
    releases
}

fn truncate(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }

    let mut out = String::new();
    for ch in value.chars().take(max.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[derive(Debug, Clone)]
struct ProbeRow {
    row_id: String,
    state: ReleaseState,
    tag: String,
    version: String,
    published: String,
    assets_count: usize,
    top_candidate: String,
    candidates: Option<Vec<AssetCandidate>>,
    candidate_error: Option<String>,
}

#[derive(Debug, Clone)]
enum ReleaseState {
    Release,
    Preview,
    Draft,
    DraftPre,
}

impl ReleaseState {
    fn label(&self) -> &'static str {
        match self {
            ReleaseState::Release => "release",
            ReleaseState::Preview => "preview",
            ReleaseState::Draft => "draft",
            ReleaseState::DraftPre => "draft+pre",
        }
    }
}

struct ProbeColumnWidths {
    id: usize,
    state: usize,
    tag: usize,
    version: usize,
    published: usize,
    assets: usize,
    top_candidate: usize,
}

impl ProbeColumnWidths {
    fn from_rows(rows: &[ProbeRow]) -> Self {
        let id = rows
            .iter()
            .map(|r| r.row_id.chars().count())
            .max()
            .unwrap_or(2)
            .max("ID".len());
        let state = rows
            .iter()
            .map(|r| r.state.label().chars().count())
            .max()
            .unwrap_or(5)
            .max("State".len());
        let tag = rows
            .iter()
            .map(|r| r.tag.chars().count())
            .max()
            .unwrap_or(3)
            .max("Tag".len())
            .min(42);
        let version = rows
            .iter()
            .map(|r| r.version.chars().count())
            .max()
            .unwrap_or(7)
            .max("Version".len())
            .min(22);
        let published = rows
            .iter()
            .map(|r| r.published.chars().count())
            .max()
            .unwrap_or(9)
            .max("Published".len());
        let assets = rows
            .iter()
            .map(|r| r.assets_count.to_string().chars().count())
            .max()
            .unwrap_or(1)
            .max("Assets".len());
        let top_candidate = rows
            .iter()
            .map(|r| r.top_candidate.chars().count())
            .max()
            .unwrap_or(13)
            .max("Top Candidate".len())
            .min(44);

        Self {
            id,
            state,
            tag,
            version,
            published,
            assets,
            top_candidate,
        }
    }

    fn table_width(&self) -> usize {
        self.id
            + self.state
            + self.tag
            + self.version
            + self.published
            + self.assets
            + self.top_candidate
            + 6 // spaces between 7 columns
    }
}
