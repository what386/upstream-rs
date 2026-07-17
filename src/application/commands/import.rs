use crate::{
    application::{
        context::CommandContext,
        operations::import_op::{
            ImportOperation, ImportPackageResult, ImportProgressEvent, ImportSummary,
        },
    },
    output::{self, Status},
    providers::provider_manager::ProviderManager,
    services::packaging::{OperationPhase, PackageProgressEvent},
    storage::{
        database::PackageDatabase,
        system::{auth::AuthStorage, config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, bail};
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

const IMPORT_PROGRESS_BAR_WIDTH: usize = 14;

fn render_import_progress(
    active_rows: &BTreeMap<String, String>,
    completed: u32,
    total: u32,
) -> String {
    let queued = total
        .saturating_sub(completed)
        .saturating_sub(active_rows.len() as u32);
    if active_rows.is_empty() {
        return format!("Importing {completed}/{total} packages ({queued} queued)");
    }
    format!(
        "Importing {completed}/{total} packages ({queued} queued)\n{}",
        active_rows.values().cloned().collect::<Vec<_>>().join("\n")
    )
}

fn render_import_progress_row(
    name: &str,
    event: PackageProgressEvent,
    name_width: usize,
) -> String {
    let detail = match event {
        PackageProgressEvent::Phase(phase) => phase.label().replace(" ...", "..."),
        PackageProgressEvent::Download { downloaded, total } if total > 0 => format!(
            "Downloading {} {} / {}",
            output::progress_bar(downloaded, total, IMPORT_PROGRESS_BAR_WIDTH),
            HumanBytes(downloaded),
            HumanBytes(total),
        ),
        PackageProgressEvent::Download { downloaded, .. } if downloaded > 0 => {
            format!("Downloading {}", HumanBytes(downloaded))
        }
        PackageProgressEvent::Download { .. } => "Downloading...".to_string(),
        PackageProgressEvent::Zsync { downloaded, total } if total > 0 => format!(
            "Zsync upgrading {} {} / {}",
            output::progress_bar(downloaded, total, IMPORT_PROGRESS_BAR_WIDTH),
            HumanBytes(downloaded),
            HumanBytes(total),
        ),
        PackageProgressEvent::Zsync { downloaded, .. } if downloaded > 0 => {
            format!("Zsync upgrading {}", HumanBytes(downloaded))
        }
        PackageProgressEvent::Zsync { .. } => "Zsync upgrading...".to_string(),
        PackageProgressEvent::Checksum { checked, total } if total > 0 => format!(
            "Checksumming {} {} / {}",
            output::progress_bar(checked, total, IMPORT_PROGRESS_BAR_WIDTH),
            HumanBytes(checked),
            HumanBytes(total),
        ),
        PackageProgressEvent::Checksum { checked, .. } if checked > 0 => {
            format!("Checksumming {}", HumanBytes(checked))
        }
        PackageProgressEvent::Checksum { .. } => "Checksumming...".to_string(),
        PackageProgressEvent::Warning(message) => output::truncate_end(&message, 96),
    };
    format!("{name:<name_width$} {detail}")
}

pub async fn run_import_packages(path: PathBuf, skip_failed: bool, latest: bool) -> Result<()> {
    let context = CommandContext::new()?;
    let mut package_database = context.package_database()?;
    let trusted_keys = context.trusted_keys()?;
    let mut import_op = ImportOperation::new(
        &context.provider_manager,
        &mut package_database,
        &context.paths,
        trusted_keys,
        context.app_config.upgrade.install_concurrency(),
    );
    let pb = new_import_progress_bar();
    let mut progress_callback = Some(new_import_progress_callback(&pb, 0));

    println!("{}", output::title("Import packages"));
    output::action_note(format!("Source: {}", path.display()));
    let result = import_op
        .import_packages(&path, skip_failed, latest, &mut progress_callback)
        .await;

    pb.finish_and_clear();
    let summary = result?;
    print_import_summary("Package import", summary);
    Ok(())
}

pub fn run_import_keys(path: PathBuf) -> Result<()> {
    let context = CommandContext::new()?;
    let mut package_database = context.package_database()?;
    let trusted_keys = context.trusted_keys()?;
    let import_op = ImportOperation::new(
        &context.provider_manager,
        &mut package_database,
        &context.paths,
        trusted_keys,
        context.app_config.upgrade.install_concurrency(),
    );
    let pb = new_import_progress_bar();
    let mut progress_callback = Some(new_import_progress_callback(&pb, 0));

    println!("{}", output::title("Import keys"));
    output::action_note(format!("Source: {}", path.display()));
    import_op.import_keys(&path, &mut progress_callback)?;

    pb.finish_and_clear();
    println!("{}", output::success("Key import complete."));
    Ok(())
}

pub fn run_import_config(path: PathBuf) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let pb = new_import_progress_bar();
    let mut progress_callback = Some(new_import_progress_callback(&pb, 0));

    println!("{}", output::title("Import config"));
    output::action_note(format!("Source: {}", path.display()));
    if !path.exists() {
        bail!("Config import source '{}' does not exist", path.display());
    }
    emit_progress(
        &mut progress_callback,
        ImportProgressEvent::Phase(OperationPhase::ImportingConfig),
    );
    let imported = ConfigStorage::new(&path)?;
    let mut target = ConfigStorage::new(&paths.config.config_file)?;
    target.replace_config(imported.get_config().clone())?;
    emit_progress(
        &mut progress_callback,
        ImportProgressEvent::Detail("Config import complete".to_string()),
    );

    pb.finish_and_clear();
    println!("{}", output::success("Config import complete."));
    Ok(())
}

pub async fn run_import_profile(path: PathBuf, skip_failed: bool, latest: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let profile_config = ImportOperation::read_profile_config(&path)?;
    let auth = AuthStorage::new(&paths.config.auth_file)?;
    let provider_manager = ProviderManager::new(
        auth.get_auth().github.api_token.as_deref(),
        auth.get_auth().gitlab.api_token.as_deref(),
        auth.get_auth().gitea.api_token.as_deref(),
        profile_config.download,
    )?;
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let trusted_keys = TrustStorage::new(&paths.config.trust_file)?.trusted_signature_keys();
    let mut import_op = ImportOperation::new(
        &provider_manager,
        &mut package_database,
        &paths,
        trusted_keys,
        profile_config.upgrade.install_concurrency(),
    );
    let pb = new_import_progress_bar();
    let mut progress_callback = Some(new_import_progress_callback(&pb, 0));

    println!("{}", output::title("Import profile"));
    output::action_note(format!("Source: {}", path.display()));
    let result = import_op
        .import_profile(&path, skip_failed, latest, &mut progress_callback)
        .await;

    pb.finish_and_clear();
    let summary = result?;
    print_import_summary("Profile import", summary);
    Ok(())
}

fn new_import_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .expect("valid import progress style"),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Importing...");
    pb
}

fn new_import_progress_callback(pb: &ProgressBar, total: u32) -> impl FnMut(ImportProgressEvent) {
    let progress_pb = pb.clone();
    let mut active_rows = BTreeMap::new();
    let mut completed = 0_u32;
    let mut total = total;
    let mut active_name_width = 0_usize;
    move |event| match event {
        ImportProgressEvent::Phase(phase) => progress_pb.set_message(phase.label()),
        ImportProgressEvent::Detail(message) => progress_pb.set_message(message),
        ImportProgressEvent::Started { package_width } => {
            active_name_width = package_width;
        }
        ImportProgressEvent::Overall {
            completed: done,
            total: count,
        } => {
            completed = done;
            total = count;
            progress_pb.set_length(count as u64);
            progress_pb.set_position(done as u64);
            progress_pb.set_message(render_import_progress(&active_rows, completed, total));
        }
        ImportProgressEvent::Package { name, event } => {
            active_rows.insert(
                name.clone(),
                render_import_progress_row(&name, event, active_name_width),
            );
            progress_pb.set_message(render_import_progress(&active_rows, completed, total));
        }
        ImportProgressEvent::Warning { name, message } => {
            let row = output::status_line_text(Status::Warn, &name, message);
            progress_pb.suspend(|| println!("{row}"));
        }
        ImportProgressEvent::Complete { name, result } => {
            active_rows.remove(&name);
            let row = match result {
                ImportPackageResult::Installed { version } => {
                    output::status_line_text(Status::Ok, &name, format!("installed {version}"))
                }
                ImportPackageResult::Failed { error } => {
                    output::status_line_text(Status::Fail, &name, error)
                }
            };
            progress_pb.suspend(|| println!("{row}"));
            progress_pb.set_message(render_import_progress(&active_rows, completed, total));
        }
        ImportProgressEvent::Clear => {
            active_rows.clear();
            progress_pb.set_message(render_import_progress(&active_rows, completed, total));
        }
    }
}

fn print_import_summary(label: &str, summary: ImportSummary) {
    let message = format!(
        "{label} complete: {} installed, {} skipped, {} failed.",
        summary.installed, summary.skipped, summary.failed
    );
    println!(
        "{}",
        if summary.failed > 0 {
            output::warning(message)
        } else {
            output::success(message)
        }
    );
}

fn emit_progress<P>(progress_callback: &mut Option<P>, event: ImportProgressEvent)
where
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(event);
    }
}

#[cfg(test)]
mod tests {
    use super::{render_import_progress, render_import_progress_row};
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};
    use std::collections::BTreeMap;

    #[test]
    fn import_progress_row_renders_download_and_phase_states() {
        let download = render_import_progress_row(
            "gitui",
            PackageProgressEvent::Download {
                downloaded: 512,
                total: 1024,
            },
            5,
        );
        assert!(download.starts_with("gitui Downloading [=======>      ]"));
        assert!(download.contains('/'));

        assert_eq!(
            render_import_progress_row(
                "dz6",
                PackageProgressEvent::Phase(PackagePhase::InstallingCompletions),
                5,
            ),
            "dz6   Installing completions..."
        );
    }

    #[test]
    fn import_progress_reports_active_and_queued_packages() {
        let mut active = BTreeMap::new();
        active.insert("gitui".to_string(), "gitui Downloading...".to_string());

        assert_eq!(
            render_import_progress(&active, 1, 4),
            "Importing 1/4 packages (2 queued)\ngitui Downloading..."
        );
    }
}
