use anyhow::Result;

use crate::{storage::database::PackageDatabase, utils::static_paths::UpstreamPaths};

use super::{DoctorReport, checks};

pub async fn run(names: Vec<String>, fix: bool) -> Result<DoctorReport> {
    let paths = UpstreamPaths::new()?;

    let mut report = DoctorReport::new();
    checks::check_local_layout(&paths, &mut report);
    checks::check_completion_directories(&paths, &mut report);
    let app_config = checks::check_app_config(&paths, fix, &mut report);
    checks::check_package_metadata_file(&paths, &mut report);
    checks::check_path_integration(&paths, fix, &mut report);

    let mut package_database = if paths.config.packages_database_file.exists() {
        Some(PackageDatabase::open(&paths.config.packages_database_file)?)
    } else {
        None
    };
    let all_packages = match &package_database {
        Some(package_database) => package_database.list_packages()?,
        None => Vec::new(),
    };
    checks::check_untracked_package_artifacts(&paths, &all_packages, &mut report);
    let selected = checks::select_packages(&names, &all_packages, &mut report);

    if let Some(config) = &app_config {
        checks::check_provider_tokens(config, &all_packages, &mut report).await;
    }

    if let Some(package_database) = &mut package_database {
        checks::check_version_tag_templates(package_database, &selected, fix, &mut report).await?;
        checks::check_installed_packages(&paths, package_database, &selected, fix, &mut report)?;
    }

    Ok(report)
}
