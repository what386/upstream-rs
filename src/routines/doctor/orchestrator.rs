use anyhow::Result;

use crate::{
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};

use super::{DoctorReport, checks};

pub async fn run(names: Vec<String>, fix: bool) -> Result<DoctorReport> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let mut report = DoctorReport::new();
    checks::check_local_layout(&paths, &mut report);
    let completion_manager = checks::check_completion_directories(&paths, &mut report);
    let app_config = checks::load_app_config(&paths, &mut report);
    checks::check_package_metadata_file(&paths, &mut report);
    checks::check_path_integration(&paths, fix, &mut report);

    let all_packages = package_storage.get_all_packages().to_vec();
    checks::check_untracked_package_artifacts(&paths, &all_packages, &mut report);
    let selected = checks::select_packages(&names, &all_packages, &mut report);

    if let Some(config) = &app_config {
        checks::check_provider_tokens(config, &all_packages, &mut report).await;
    }

    checks::check_installed_packages(
        &paths,
        &mut package_storage,
        &selected,
        &completion_manager,
        fix,
        &mut report,
    )?;

    Ok(report)
}
