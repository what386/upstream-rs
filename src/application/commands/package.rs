#[cfg(target_os = "linux")]
use crate::services::artifact::AppImageExtractor;
use crate::{
    application::{cli::arguments::PackageSettingKey, operations::metadata_op::MetadataManager},
    models::{common::enums::TrustMode, upstream::Package},
    output::{self, Status},
    providers::pattern_matcher::PatternTable,
    services::integration::{DesktopManager, SymlinkManager},
    storage::database::{PackageDatabase, PackageSettings},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
struct PackageSettingsView {
    match_pattern: PatternTable,
    exclude_pattern: PatternTable,
    trust_mode: Option<TrustMode>,
    effective_trust_mode: TrustMode,
}

pub fn run_set(name: String, assignments: Vec<String>, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package = load_package(&package_database, &name)?;
    let mut settings = package_database
        .get_package_settings(&name)?
        .unwrap_or_else(|| PackageSettings::new(&name));
    let mut seen = HashSet::new();

    for assignment in assignments {
        let (raw_key, value) = assignment
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid package setting '{assignment}'; expected key=value"))?;
        let key = parse_setting_key(raw_key)?;
        if !seen.insert(key) {
            bail!(
                "Package setting '{}' was provided more than once",
                setting_name(key)
            );
        }
        if value.trim().is_empty() {
            bail!(
                "Package setting '{}' cannot be empty; use 'package unset' to clear it",
                setting_name(key)
            );
        }
        match key {
            PackageSettingKey::MatchPattern => {
                let patterns = PatternTable::from_comma_separated(value);
                if patterns.is_empty() {
                    bail!("match_pattern must contain at least one non-empty pattern");
                }
                package.match_pattern = patterns;
            }
            PackageSettingKey::ExcludePattern => {
                let patterns = PatternTable::from_comma_separated(value);
                if patterns.is_empty() {
                    bail!("exclude_pattern must contain at least one non-empty pattern");
                }
                package.exclude_pattern = patterns;
            }
            PackageSettingKey::TrustMode => {
                settings.trust_mode = Some(
                    TrustMode::from_str(value.trim(), true)
                        .map_err(|err| anyhow!("Invalid trust_mode: {err}"))?,
                )
            }
        }
    }

    package_database.upsert_package_with_settings(&package, &settings)?;
    println!("{}", output::title("Package settings"));
    output::status_line(Status::Ok, &name, "settings updated");
    Ok(())
}

pub fn run_get(
    name: String,
    keys: Vec<PackageSettingKey>,
    json: bool,
    paths: &UpstreamPaths,
) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let package = load_package(&package_database, &name)?;
    let trust_mode = package_database
        .get_package_settings(&name)?
        .and_then(|settings| settings.trust_mode);
    let view = PackageSettingsView {
        match_pattern: package.match_pattern,
        exclude_pattern: package.exclude_pattern,
        trust_mode,
        effective_trust_mode: trust_mode.unwrap_or(TrustMode::BestEffort),
    };

    let keys = selected_setting_keys(keys);
    if json {
        let mut values = serde_json::Map::new();
        for key in &keys {
            match key {
                PackageSettingKey::MatchPattern => {
                    values.insert(
                        "match_pattern".to_string(),
                        serde_json::to_value(&view.match_pattern)?,
                    );
                }
                PackageSettingKey::ExcludePattern => {
                    values.insert(
                        "exclude_pattern".to_string(),
                        serde_json::to_value(&view.exclude_pattern)?,
                    );
                }
                PackageSettingKey::TrustMode => {
                    values.insert(
                        "trust_mode".to_string(),
                        serde_json::to_value(view.trust_mode)?,
                    );
                    values.insert(
                        "effective_trust_mode".to_string(),
                        serde_json::to_value(view.effective_trust_mode)?,
                    );
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }

    println!("{}", output::title("Package settings"));
    for key in keys {
        match key {
            PackageSettingKey::MatchPattern => {
                println!("match_pattern={}", view.match_pattern)
            }
            PackageSettingKey::ExcludePattern => {
                println!("exclude_pattern={}", view.exclude_pattern)
            }
            PackageSettingKey::TrustMode => match view.trust_mode {
                Some(mode) => println!("trust_mode={mode}"),
                None => println!(
                    "trust_mode=<unset> (effective: {})",
                    view.effective_trust_mode
                ),
            },
        }
    }
    Ok(())
}

pub fn run_unset(name: String, keys: Vec<PackageSettingKey>, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package = load_package(&package_database, &name)?;
    let mut settings = package_database
        .get_package_settings(&name)?
        .unwrap_or_else(|| PackageSettings::new(&name));

    for key in selected_setting_keys(keys) {
        match key {
            PackageSettingKey::MatchPattern => package.match_pattern = PatternTable::empty(),
            PackageSettingKey::ExcludePattern => package.exclude_pattern = PatternTable::empty(),
            PackageSettingKey::TrustMode => settings.trust_mode = None,
        }
    }

    package_database.upsert_package_with_settings(&package, &settings)?;
    println!("{}", output::title("Package settings"));
    output::status_line(Status::Ok, &name, "settings cleared");
    Ok(())
}

fn load_package(package_database: &PackageDatabase, name: &str) -> Result<Package> {
    package_database
        .get_package(name)?
        .ok_or_else(|| anyhow!("Package '{}' not found", name))
}

fn parse_setting_key(raw: &str) -> Result<PackageSettingKey> {
    match raw.trim().replace('-', "_").as_str() {
        "match_pattern" => Ok(PackageSettingKey::MatchPattern),
        "exclude_pattern" => Ok(PackageSettingKey::ExcludePattern),
        "trust_mode" => Ok(PackageSettingKey::TrustMode),
        other => bail!(
            "Unknown package setting '{}'; supported settings: match_pattern, exclude_pattern, trust_mode",
            other
        ),
    }
}

fn setting_name(key: PackageSettingKey) -> &'static str {
    match key {
        PackageSettingKey::MatchPattern => "match_pattern",
        PackageSettingKey::ExcludePattern => "exclude_pattern",
        PackageSettingKey::TrustMode => "trust_mode",
    }
}

fn selected_setting_keys(keys: Vec<PackageSettingKey>) -> Vec<PackageSettingKey> {
    let keys = if keys.is_empty() {
        vec![
            PackageSettingKey::MatchPattern,
            PackageSettingKey::ExcludePattern,
            PackageSettingKey::TrustMode,
        ]
    } else {
        keys
    };
    let mut seen = HashSet::new();
    keys.into_iter().filter(|key| seen.insert(*key)).collect()
}

pub fn run_pin(name: String, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package_manager = MetadataManager::new(&mut package_database);

    println!("{}", output::title("Package pin"));

    package_manager.pin_package(&name)?;
    output::status_line(Status::Ok, &name, "pinned");

    Ok(())
}

pub fn run_unpin(name: String, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package_manager = MetadataManager::new(&mut package_database);

    println!("{}", output::title("Package unpin"));

    package_manager.unpin_package(&name)?;
    output::status_line(Status::Ok, &name, "unpinned");

    Ok(())
}

pub fn run_rename(old_name: String, new_name: String, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let package_before = package_database
        .get_package(&old_name)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", old_name))?;

    let mut package_manager = MetadataManager::new(&mut package_database);
    println!("{}", output::title("Package rename"));

    let renamed = package_manager.rename_package(&old_name, &new_name)?;
    if !renamed {
        output::status_line(Status::Skip, &old_name, "old and new names are identical");
        return Ok(());
    }

    if let Some(exec_path) = package_before.exec_path.as_ref() {
        let symlink_manager = SymlinkManager::new(&paths.state.symlinks_dir);
        let mut created_new = false;

        if let Err(err) = symlink_manager.add_link(exec_path, &new_name) {
            println!(
                "{}",
                output::warning(format!(
                    "Renamed package but failed to create new symlink '{}': {}",
                    new_name, err
                ))
            );
        } else {
            created_new = true;
        }

        if created_new && let Err(err) = symlink_manager.remove_link(&old_name) {
            println!(
                "{}",
                output::warning(format!(
                    "Renamed package but failed to remove old symlink '{}': {}",
                    old_name, err
                ))
            );
        }
    }

    println!(
        "{}",
        output::success(format!("Package '{}' renamed to '{}'.", old_name, new_name))
    );
    Ok(())
}

pub async fn run_add_entry(name: String, paths: &UpstreamPaths) -> Result<()> {
    let (mut package_database, mut package) = load_installed_package(&name, paths)?;

    #[cfg(target_os = "linux")]
    let appimage_extractor =
        AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

    #[cfg(target_os = "linux")]
    let desktop_manager = DesktopManager::new(paths, &appimage_extractor);
    #[cfg(not(target_os = "linux"))]
    let desktop_manager = DesktopManager::new(&paths);

    println!("{}", output::title("Package add-entry"));

    let mut ignored_messages = Some(|_: &str| {});
    desktop_manager
        .enable_package_entry(&mut package, &mut ignored_messages)
        .await?;

    save_package(&mut package_database, &package)?;
    output::status_line(Status::Ok, &name, "entry added");

    Ok(())
}

pub async fn run_rm_entry(name: String, paths: &UpstreamPaths) -> Result<()> {
    let (mut package_database, mut package) = load_installed_package(&name, paths)?;

    #[cfg(target_os = "linux")]
    let appimage_extractor =
        AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

    #[cfg(target_os = "linux")]
    let desktop_manager = DesktopManager::new(paths, &appimage_extractor);
    #[cfg(not(target_os = "linux"))]
    let desktop_manager = DesktopManager::new(&paths);

    println!("{}", output::title("Package rm-entry"));

    let mut ignored_messages = Some(|_: &str| {});
    desktop_manager.disable_package_entry(&mut package, &mut ignored_messages)?;

    save_package(&mut package_database, &package)?;
    output::status_line(Status::Ok, &name, "entry removed");

    Ok(())
}

fn load_installed_package(name: &str, paths: &UpstreamPaths) -> Result<(PackageDatabase, Package)> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let package = package_database
        .get_package(name)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

    Ok((package_database, package))
}

fn save_package(package_database: &mut PackageDatabase, package: &Package) -> Result<()> {
    package_database.upsert_package(package).context(format!(
        "Failed to save package '{}' to storage",
        package.name
    ))
}
