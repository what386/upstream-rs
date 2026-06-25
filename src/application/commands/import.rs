use crate::{
    application::{context::CommandContext, operations::import_op::ImportOperation},
    output,
    providers::provider_manager::ProviderManager,
    services::packaging::OperationProgressEvent,
    storage::{
        database::PackageDatabase,
        system::{config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

fn render_import_progress(event: OperationProgressEvent) -> String {
    match event {
        OperationProgressEvent::Phase(phase) => phase.label().to_string(),
        OperationProgressEvent::Count { done, total } => format!("Importing ... {done}/{total}"),
        OperationProgressEvent::Warning(message) | OperationProgressEvent::Detail(message) => {
            message
        }
    }
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
    );
    let pb = new_import_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_import_progress(event));
    });

    println!("{}", output::title("Import packages"));
    output::action_note(format!("Source: {}", path.display()));
    import_op
        .import_packages(&path, skip_failed, latest, &mut progress_callback)
        .await?;

    pb.finish_and_clear();
    println!("{}", output::success("Package import complete."));
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
    );
    let pb = new_import_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_import_progress(event));
    });

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
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_import_progress(event));
    });

    println!("{}", output::title("Import config"));
    output::action_note(format!("Source: {}", path.display()));
    if !path.exists() {
        bail!("Config import source '{}' does not exist", path.display());
    }
    emit_progress(
        &mut progress_callback,
        OperationProgressEvent::Phase(crate::services::packaging::OperationPhase::ImportingConfig),
    );
    let imported = ConfigStorage::new(&path)?;
    let mut target = ConfigStorage::new(&paths.config.config_file)?;
    target.replace_config(imported.get_config().clone())?;
    emit_progress(
        &mut progress_callback,
        OperationProgressEvent::Detail("Config import complete".to_string()),
    );

    pb.finish_and_clear();
    println!("{}", output::success("Config import complete."));
    Ok(())
}

pub async fn run_import_profile(path: PathBuf, skip_failed: bool, latest: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let profile_config = ImportOperation::read_profile_config(&path)?;
    let provider_manager = ProviderManager::new(
        profile_config.github.api_token.as_deref(),
        profile_config.gitlab.api_token.as_deref(),
        profile_config.gitea.api_token.as_deref(),
        profile_config.download,
    )?;
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let trusted_keys = TrustStorage::new(&paths.config.trust_file)?.trusted_signature_keys();
    let mut import_op = ImportOperation::new(
        &provider_manager,
        &mut package_database,
        &paths,
        trusted_keys,
    );
    let pb = new_import_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_import_progress(event));
    });

    println!("{}", output::title("Import profile"));
    output::action_note(format!("Source: {}", path.display()));
    import_op
        .import_profile(&path, skip_failed, latest, &mut progress_callback)
        .await?;

    pb.finish_and_clear();
    println!("{}", output::success("Profile import complete."));
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
    pb.set_message("Importing ...");
    pb
}

fn emit_progress<P>(progress_callback: &mut Option<P>, event: OperationProgressEvent)
where
    P: FnMut(OperationProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(event);
    }
}
