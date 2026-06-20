use anyhow::{Result, anyhow};
use console::style;
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;

use crate::{
    application::operations::install_operation::{
        InstallOperation, PackageTransactionContext, SelectedAssetInstallRequest,
    },
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        provider::{Asset, Release},
        upstream::Package,
    },
    output::{self, Status, TransactionRow, pager},
    providers::discovery::{DiscoveryRequest, infer_package_name},
    providers::{asset_selector::AssetCandidate, provider_manager::ProviderManager},
    services::{
        packaging::{
            PackagePhase, PackageProgressEvent,
            disk_impact::{asset_size_estimate, install_impact_from_download},
        },
        storage::{
            config_storage::ConfigStorage, package_storage::PackageStorage,
            trust_storage::TrustStorage,
        },
    },
    utils::static_paths::UpstreamPaths,
};

const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

#[allow(clippy::too_many_arguments)]
pub async fn run(
    repo_slug: String,
    name: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    limit: u32,
    verbose: bool,
    json: bool,
    create_entry: bool,
    trust_mode: TrustMode,
    dry_run: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();

    let provider_manager =
        ProviderManager::new(github_token, gitlab_token, gitea_token, app_config.download)?;

    let mut probe_notes = Vec::new();
    let (effective_repo_slug, effective_provider, effective_base_url, mut releases) =
        if let Some(provider) = provider {
            probe_notes.push(format!("Probing '{}' via {}", repo_slug, provider));

            let releases = provider_manager
                .get_releases(
                    &repo_slug,
                    &provider,
                    Some(limit),
                    Some(limit),
                    base_url.as_deref(),
                )
                .await?;
            (repo_slug.clone(), provider, base_url.clone(), releases)
        } else {
            let discovery = provider_manager
                .discover_source(DiscoveryRequest {
                    source: repo_slug.clone(),
                    channel: channel.clone(),
                    package_name: String::new(),
                    filetype: Filetype::Auto,
                    match_pattern: None,
                    exclude_pattern: None,
                    base_url_override: base_url.clone(),
                    limit,
                })
                .await?;

            probe_notes.push(format!(
                "Probing '{}' as '{}' via {}",
                repo_slug, discovery.source.repo_slug, discovery.source.provider
            ));

            (
                discovery.source.repo_slug,
                discovery.source.provider,
                discovery.source.base_url,
                discovery.releases,
            )
        };

    releases = filter_by_channel(releases, &channel);
    releases.sort_by(|a, b| b.version.cmp(&a.version));

    if releases.is_empty() {
        if json {
            let result = json_probe_result(
                &repo_slug,
                &effective_repo_slug,
                &effective_provider,
                effective_base_url.as_deref(),
                &channel,
                probe_notes,
                &[],
                verbose,
            );
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        println!(
            "{}",
            crate::output::warning(format!("No releases found for channel '{}'.", channel))
        );
        return Ok(());
    }

    let probe_package = Package::with_defaults(
        String::new(),
        effective_repo_slug.clone(),
        Filetype::Auto,
        None,
        None,
        channel.clone(),
        effective_provider.clone(),
        effective_base_url.clone(),
    );

    let rows = build_probe_rows(&releases, &provider_manager, &probe_package);
    if json {
        let result = json_probe_result(
            &repo_slug,
            &effective_repo_slug,
            &effective_provider,
            effective_base_url.as_deref(),
            &channel,
            probe_notes,
            &rows,
            verbose,
        );
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if dry_run {
        pager::page_text(
            Some("Probe"),
            &format_probe_results(&probe_notes, &rows, verbose),
        )?;
        return Ok(());
    }

    let choices = build_probe_asset_choices(&releases, &provider_manager, &probe_package);
    if choices.is_empty() {
        println!(
            "{}",
            output::warning(format!("No assets found for channel '{}'.", channel))
        );
        return Ok(());
    }

    let table = ProbeAssetChoiceTable::from_choices(&choices);
    let prompt = format!(
        "Probe: '{}' as '{}' via {}",
        repo_slug, effective_repo_slug, effective_provider
    );
    let Some(selected) = output::select_from_table(prompt, &table.headers, &table.rows)? else {
        println!("{}", output::warning("Cancelled"));
        return Ok(());
    };

    let selected_choice = &choices[selected];
    let selected_release = releases
        .get(selected_choice.release_index)
        .cloned()
        .ok_or_else(|| anyhow!("Selected release no longer exists"))?;
    let selected_asset = selected_choice.asset.clone();

    let install_name = resolve_probe_package_name(
        name,
        &effective_repo_slug,
        &effective_provider,
        effective_base_url.as_deref(),
    )?;
    let selector = crate::providers::asset_selector::AssetSelector::new();
    let generated = selector.generate_patterns_for_asset(
        &selected_asset,
        &selected_release.assets,
        &install_name,
    );

    let package = Package::with_defaults(
        install_name.clone(),
        effective_repo_slug,
        selected_asset.filetype,
        Some(generated.match_pattern.to_string()),
        Some(generated.exclude_pattern.to_string()),
        channel,
        effective_provider,
        effective_base_url,
    );

    let preview_impact = install_impact_from_download(asset_size_estimate(selected_asset.size));
    println!("{}", output::title("Install preview"));
    output::kv("Package", &package.name);
    output::kv(
        "Source",
        format!("{} ({})", package.repo_slug, package.provider),
    );
    output::kv(
        "Release",
        format!("{} ({})", selected_release.name, selected_release.tag),
    );
    output::kv(
        "Asset",
        format!("{} ({:?})", selected_asset.name, selected_asset.filetype),
    );
    output::kv(
        "Match",
        if package.match_pattern.is_empty() {
            "-".to_string()
        } else {
            package.match_pattern.to_string()
        },
    );
    output::kv(
        "Exclude",
        if package.exclude_pattern.is_empty() {
            "-".to_string()
        } else {
            package.exclude_pattern.to_string()
        },
    );
    output::kv("Trust", trust_mode);
    output::kv("Desktop", if create_entry { "yes" } else { "no" });
    output::print_disk_impact(&preview_impact, true);

    let transaction_rows = vec![TransactionRow::single_version(
        format!("{}/{}", package.provider, package.name),
        &selected_release.tag,
        preview_impact.net,
        preview_impact.download,
    )];
    output::print_transaction_table(&transaction_rows, &preview_impact, "Net disk change:");
    output::confirm_or_cancel("Proceed with installation?", true)?;

    let trust_storage = TrustStorage::new(&paths.config.trust_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let trusted_keys = trust_storage.trusted_signature_keys();
    let mut install_operation = InstallOperation::new(
        &provider_manager,
        &mut package_storage,
        &paths,
        trusted_keys,
    )?;

    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message(format!("Installing {}", package.name));

    let progress_name = package.name.clone();
    let install_name = package.name.clone();
    let install_version = selected_release.tag.clone();
    let progress_pb = pb.clone();
    let mut last_emit = None;
    let mut progress_callback = Some(move |event: PackageProgressEvent| {
        let should_emit = last_emit
            .map(|elapsed: std::time::Instant| elapsed.elapsed() >= PROGRESS_UPDATE_INTERVAL)
            .unwrap_or(true);
        if should_emit || !matches!(event, PackageProgressEvent::Download { .. }) {
            progress_pb.set_message(render_probe_install_progress_message(&progress_name, event));
            last_emit = Some(std::time::Instant::now());
        }
    });

    let mut no_download_progress: Option<fn(u64, u64)> = None;
    let mut ignored_messages = Some(|_: &str| {});
    let install_result = install_operation
        .install_selected_asset(
            SelectedAssetInstallRequest {
                package,
                release: &selected_release,
                asset: &selected_asset,
                add_entry: create_entry,
                trust_mode,
                transaction_context: PackageTransactionContext::install(),
            },
            &mut no_download_progress,
            &mut ignored_messages,
            &mut progress_callback,
        )
        .await;

    pb.finish_and_clear();

    match install_result {
        Ok(_) => {
            println!(
                "{}",
                output::status_line_text(
                    Status::Ok,
                    &install_name,
                    format!("installed {install_version}")
                )
            );
            println!(
                "{}",
                output::success("Install complete: 1 installed, 0 failed.")
            );
        }
        Err(err) => {
            println!(
                "{}",
                output::status_line_text(Status::Fail, &install_name, output::error_summary(&err))
            );
            println!(
                "{}",
                output::warning("Install complete: 0 installed, 1 failed.")
            );
        }
    }

    Ok(())
}

fn resolve_probe_package_name(
    name: Option<String>,
    source: &str,
    provider: &Provider,
    base_url: Option<&str>,
) -> Result<String> {
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        return Ok(name);
    }

    let default = infer_package_name(source, Some(provider), base_url)?;
    output::prompt_text("Package name", default.as_deref())
}

fn render_probe_install_progress_message(name: &str, event: PackageProgressEvent) -> String {
    format!(
        "Installing {name}\n{}",
        render_probe_install_progress_row(name, event)
    )
}

fn render_probe_install_progress_row(name: &str, event: PackageProgressEvent) -> String {
    match event {
        PackageProgressEvent::Phase(phase) => {
            format!(" {:<28} {}", name, phase.label())
        }
        PackageProgressEvent::Download { downloaded, total } => {
            let transfer = if total > 0 {
                format!("{} / {}", HumanBytes(downloaded), HumanBytes(total))
            } else if downloaded > 0 {
                format!("{}", HumanBytes(downloaded))
            } else {
                "-".to_string()
            };
            format!(
                " {:<28} {:<28} {}",
                name,
                PackagePhase::DownloadingPackage.label(),
                transfer
            )
        }
        PackageProgressEvent::Warning(message) => {
            format!(" {:<28} {}", name, message)
        }
    }
}

fn build_probe_asset_choices(
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

#[derive(Debug, Clone)]
struct ProbeAssetChoice {
    release_index: usize,
    release_tag: String,
    release_state: ReleaseState,
    asset: Asset,
    score: Option<i32>,
}

struct ProbeAssetChoiceTable {
    headers: Vec<String>,
    rows: Vec<String>,
}

impl ProbeAssetChoiceTable {
    fn from_choices(choices: &[ProbeAssetChoice]) -> Self {
        let widths = ProbeAssetChoiceWidths::from_choices(choices);
        let header = format!(
            "  {:<rel$} {:<state$} {:<name$} {:<kind$} {:>size$} {:<os$} {:<arch$} {:>score$}",
            "Release",
            "State",
            "Asset",
            "Kind",
            "Size",
            "OS",
            "Arch",
            "Score",
            rel = widths.release,
            state = widths.state,
            name = widths.asset,
            kind = widths.kind,
            size = widths.size,
            os = widths.os,
            arch = widths.arch,
            score = widths.score,
        );
        let divider = format!("  {}", output::divider(widths.table_width()));
        let rows = choices
            .iter()
            .map(|choice| format_probe_asset_choice(choice, &widths))
            .collect();

        Self {
            headers: vec![header, divider],
            rows,
        }
    }
}

fn format_probe_asset_choice(choice: &ProbeAssetChoice, widths: &ProbeAssetChoiceWidths) -> String {
    let asset = &choice.asset;
    format!(
        "{:<rel$} {:<state$} {:<name$} {:<kind$} {:>size$} {:<os$} {:<arch$} {:>score$}",
        truncate(&choice.release_tag, widths.release),
        choice.release_state.label(),
        truncate(&asset.name, widths.asset),
        truncate(&format!("{:?}", asset.filetype), widths.kind),
        HumanBytes(asset.size),
        asset
            .target_os
            .as_ref()
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "-".to_string()),
        asset
            .target_arch
            .as_ref()
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "-".to_string()),
        choice
            .score
            .map(|score| score.to_string())
            .unwrap_or_else(|| "-".to_string()),
        rel = widths.release,
        state = widths.state,
        name = widths.asset,
        kind = widths.kind,
        size = widths.size,
        os = widths.os,
        arch = widths.arch,
        score = widths.score,
    )
}

struct ProbeAssetChoiceWidths {
    release: usize,
    state: usize,
    asset: usize,
    kind: usize,
    size: usize,
    os: usize,
    arch: usize,
    score: usize,
}

impl ProbeAssetChoiceWidths {
    fn from_choices(choices: &[ProbeAssetChoice]) -> Self {
        let release = choices
            .iter()
            .map(|choice| choice.release_tag.chars().count())
            .max()
            .unwrap_or(7)
            .max("Release".len())
            .min(28);
        let state = choices
            .iter()
            .map(|choice| choice.release_state.label().chars().count())
            .max()
            .unwrap_or(5)
            .max("State".len());
        let asset = choices
            .iter()
            .map(|choice| choice.asset.name.chars().count())
            .max()
            .unwrap_or(5)
            .max("Asset".len())
            .min(56);
        let kind = choices
            .iter()
            .map(|choice| format!("{:?}", choice.asset.filetype).chars().count())
            .max()
            .unwrap_or(4)
            .max("Kind".len())
            .min(20);
        let size = choices
            .iter()
            .map(|choice| HumanBytes(choice.asset.size).to_string().chars().count())
            .max()
            .unwrap_or(4)
            .max("Size".len());
        let os = choices
            .iter()
            .map(|choice| {
                choice
                    .asset
                    .target_os
                    .as_ref()
                    .map(|value| format!("{value:?}").chars().count())
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or(2)
            .max("OS".len())
            .min(10);
        let arch = choices
            .iter()
            .map(|choice| {
                choice
                    .asset
                    .target_arch
                    .as_ref()
                    .map(|value| format!("{value:?}").chars().count())
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or(4)
            .max("Arch".len())
            .min(12);
        let score = choices
            .iter()
            .map(|choice| {
                choice
                    .score
                    .map(|score| score.to_string().chars().count())
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or(5)
            .max("Score".len());

        Self {
            release,
            state,
            asset,
            kind,
            size,
            os,
            arch,
            score,
        }
    }

    fn table_width(&self) -> usize {
        self.release
            + self.state
            + self.asset
            + self.kind
            + self.size
            + self.os
            + self.arch
            + self.score
            + 7
    }
}

#[derive(Serialize)]
struct JsonProbeResult {
    source: JsonProbeSource,
    channel: String,
    notes: Vec<String>,
    releases: Vec<JsonProbeRelease>,
}

#[derive(Serialize)]
struct JsonProbeSource {
    input: String,
    repo_slug: String,
    provider: String,
    base_url: Option<String>,
}

#[derive(Serialize)]
struct JsonProbeRelease {
    id: String,
    state: &'static str,
    tag: String,
    version: String,
    published: String,
    assets_count: usize,
    top_candidate: String,
    candidates: Option<Vec<JsonAssetCandidate>>,
    candidate_error: Option<String>,
}

#[derive(Serialize)]
struct JsonAssetCandidate {
    rank: usize,
    score: i32,
    id: u64,
    name: String,
    download_url: String,
    size: u64,
    created_at: String,
    filetype: String,
    target_os: Option<String>,
    target_arch: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn json_probe_result(
    input: &str,
    repo_slug: &str,
    provider: &Provider,
    base_url: Option<&str>,
    channel: &Channel,
    notes: Vec<String>,
    rows: &[ProbeRow],
    include_candidates: bool,
) -> JsonProbeResult {
    JsonProbeResult {
        source: JsonProbeSource {
            input: input.to_string(),
            repo_slug: repo_slug.to_string(),
            provider: provider.to_string(),
            base_url: base_url.map(str::to_string),
        },
        channel: channel.to_string(),
        notes,
        releases: rows
            .iter()
            .map(|row| JsonProbeRelease {
                id: row.row_id.clone(),
                state: row.state.label(),
                tag: row.tag.clone(),
                version: row.version.clone(),
                published: row.published.clone(),
                assets_count: row.assets_count,
                top_candidate: row.top_candidate.clone(),
                candidates: include_candidates.then(|| json_asset_candidates(row)),
                candidate_error: row.candidate_error.clone(),
            })
            .collect(),
    }
}

fn json_asset_candidates(row: &ProbeRow) -> Vec<JsonAssetCandidate> {
    row.candidates
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let asset = &candidate.asset;
            JsonAssetCandidate {
                rank: idx + 1,
                score: candidate.score,
                id: asset.id,
                name: asset.name.clone(),
                download_url: asset.download_url.clone(),
                size: asset.size,
                created_at: asset.created_at.to_rfc3339(),
                filetype: asset.filetype.to_string(),
                target_os: asset.target_os.as_ref().map(|value| format!("{value:?}")),
                target_arch: asset.target_arch.as_ref().map(|value| format!("{value:?}")),
            }
        })
        .collect()
}

fn write_candidates(out: &mut String, row: &ProbeRow) {
    let Some(candidates) = row.candidates.as_ref() else {
        writeln!(
            out,
            "     candidates: failed ({})",
            truncate(row.candidate_error.as_deref().unwrap_or("unknown"), 48)
        )
        .expect("write candidate error");
        return;
    };

    if candidates.is_empty() {
        writeln!(out, "     candidates: none").expect("write empty candidates");
        return;
    }

    writeln!(out, "     candidates:").expect("write candidates label");
    for (rank, candidate) in candidates.iter().take(6).enumerate() {
        let asset = &candidate.asset;
        writeln!(
            out,
            "       #{} {:<44} {:>11} {:<10} score={}",
            rank + 1,
            truncate(&asset.name, 46),
            HumanBytes(asset.size),
            format!("{:?}", asset.filetype),
            candidate.score
        )
        .expect("write candidate row");
    }
    if candidates.len() > 6 {
        writeln!(out, "       ... and {} more", candidates.len() - 6)
            .expect("write candidate overflow");
    }
}

fn format_probe_results(notes: &[String], rows: &[ProbeRow], verbose: bool) -> String {
    let widths = ProbeColumnWidths::from_rows(rows);
    let mut out = String::new();

    for note in notes {
        writeln!(out, "  {note}").expect("write probe note");
    }
    if !notes.is_empty() {
        writeln!(out).expect("write probe note spacer");
    }

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
    writeln!(out, "{}", style(header).bold()).expect("write probe header");
    writeln!(out, "{}", "-".repeat(widths.table_width())).expect("write probe divider");

    for row in rows {
        writeln!(
            out,
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
        )
        .expect("write probe row");

        if verbose {
            write_candidates(&mut out, row);
        }
    }

    out
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

#[cfg(test)]
mod tests {
    use super::{
        JsonProbeResult, ProbeAssetChoiceTable, ProbeRow, ReleaseState, build_probe_asset_choices,
        json_probe_result,
    };
    use crate::{
        models::{
            common::{
                Version,
                enums::{Channel, Filetype, Provider},
            },
            provider::{Asset, Release},
            upstream::Package,
        },
        providers::{asset_selector::AssetCandidate, provider_manager::ProviderManager},
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn json_probe_result_includes_source_releases_and_candidates() {
        let created_at = chrono::Utc.with_ymd_and_hms(2026, 6, 12, 1, 2, 3).unwrap();
        let row = ProbeRow {
            row_id: "R01".to_string(),
            state: ReleaseState::Release,
            tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            published: "2026-06-12 01:02".to_string(),
            assets_count: 1,
            top_candidate: "tool.tar.gz (42)".to_string(),
            candidates: Some(vec![AssetCandidate {
                asset: Asset {
                    download_url: "https://example.invalid/tool.tar.gz".to_string(),
                    id: 7,
                    name: "tool.tar.gz".to_string(),
                    size: 1234,
                    created_at,
                    filetype: Filetype::Archive,
                    target_os: None,
                    target_arch: None,
                },
                score: 42,
            }]),
            candidate_error: None,
        };

        let result: JsonProbeResult = json_probe_result(
            "owner/tool",
            "owner/tool",
            &Provider::Github,
            None,
            &Channel::Stable,
            vec!["Probing 'owner/tool' via github".to_string()],
            &[row],
            true,
        );
        let json = serde_json::to_value(result).expect("serialize probe result");

        assert_eq!(json["source"]["provider"], "github");
        assert_eq!(json["channel"], "Stable");
        assert_eq!(json["releases"][0]["state"], "release");
        assert_eq!(json["releases"][0]["candidates"][0]["rank"], 1);
        assert_eq!(
            json["releases"][0]["candidates"][0]["filetype"],
            "Compressed archive"
        );
    }

    #[test]
    fn probe_asset_choices_include_all_release_assets() {
        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let package = Package::with_defaults(
            String::new(),
            "owner/tool".to_string(),
            Filetype::Auto,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let releases = vec![Release {
            id: 1,
            tag: "v1.2.3".to_string(),
            name: "v1.2.3".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![
                Asset::new(
                    "https://example.invalid/tool-linux-x86_64.tar.gz".to_string(),
                    1,
                    "tool-linux-x86_64.tar.gz".to_string(),
                    1234,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-debug-symbols.zip".to_string(),
                    2,
                    "tool-debug-symbols.zip".to_string(),
                    5678,
                    Utc::now(),
                ),
            ],
            version: Version::new(1, 2, 3, false),
            published_at: Utc::now(),
        }];

        let choices = build_probe_asset_choices(&releases, &provider_manager, &package);
        let table = ProbeAssetChoiceTable::from_choices(&choices);

        assert_eq!(choices.len(), 2);
        assert!(table.rows[0].contains("tool-linux-x86_64.tar.gz"));
        assert!(table.rows[1].contains("tool-debug-symbols.zip"));
    }
}
