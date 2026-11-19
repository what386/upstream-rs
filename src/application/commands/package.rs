use crate::{
    application::{
        cli::arguments::PackageSettingKey,
        operations::{package_op, rename_op::RenameOutcome},
    },
    models::common::enums::TrustMode,
    output::{self, Status},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;

pub fn run_set(name: String, assignments: Vec<String>, paths: &UpstreamPaths) -> Result<()> {
    package_op::set(&name, assignments, paths)?;
    println!("{}", output::title("Package settings"));
    output::status_line(Status::Ok, &name, "settings updated");
    Ok(())
}

pub fn run_unset(name: String, keys: Vec<PackageSettingKey>, paths: &UpstreamPaths) -> Result<()> {
    package_op::unset(&name, keys, paths)?;
    println!("{}", output::title("Package settings"));
    output::status_line(Status::Ok, &name, "settings cleared");
    Ok(())
}

pub fn run_get(
    name: String,
    keys: Vec<PackageSettingKey>,
    json: bool,
    paths: &UpstreamPaths,
) -> Result<()> {
    let (package, settings) = package_op::get(&name, paths)?;
    let keys = package_op::selected_setting_keys(keys);
    let mut values = serde_json::Map::new();
    for key in &keys {
        match key {
            PackageSettingKey::MatchPattern => {
                values.insert(
                    "match_pattern".into(),
                    serde_json::to_value(&package.match_pattern)?,
                );
            }
            PackageSettingKey::ExcludePattern => {
                values.insert(
                    "exclude_pattern".into(),
                    serde_json::to_value(&package.exclude_pattern)?,
                );
            }
            PackageSettingKey::TrustMode => {
                values.insert(
                    "trust_mode".into(),
                    serde_json::to_value(settings.trust_mode)?,
                );
                values.insert(
                    "effective_trust_mode".into(),
                    serde_json::to_value(settings.trust_mode.unwrap_or(TrustMode::BestEffort))?,
                );
            }
            PackageSettingKey::DesktopLaunchOptions => {
                values.insert(
                    "desktop_launch_options".into(),
                    serde_json::to_value(&settings.desktop_launch_options)?,
                );
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }

    println!("{}", output::title("Package settings"));
    for key in keys {
        match key {
            PackageSettingKey::MatchPattern => println!("match_pattern={}", package.match_pattern),
            PackageSettingKey::ExcludePattern => {
                println!("exclude_pattern={}", package.exclude_pattern)
            }
            PackageSettingKey::TrustMode => match settings.trust_mode {
                Some(mode) => println!("trust_mode={mode}"),
                None => println!("trust_mode=<unset> (effective: {})", TrustMode::BestEffort),
            },
            PackageSettingKey::DesktopLaunchOptions => println!(
                "desktop_launch_options={}",
                settings
                    .desktop_launch_options
                    .as_deref()
                    .unwrap_or("<unset>")
            ),
        }
    }

    Ok(())
}

pub fn run_pin(name: String, paths: &UpstreamPaths) -> Result<()> {
    package_op::pin(&name, true, paths)?;
    println!("{}", output::title("Package pin"));
    output::status_line(Status::Ok, &name, "pinned");
    Ok(())
}

pub fn run_unpin(name: String, paths: &UpstreamPaths) -> Result<()> {
    package_op::pin(&name, false, paths)?;
    println!("{}", output::title("Package unpin"));
    output::status_line(Status::Ok, &name, "unpinned");
    Ok(())
}

pub fn run_rename(old_name: String, new_name: String, paths: &UpstreamPaths) -> Result<()> {
    let outcome = package_op::rename(&old_name, &new_name, paths)?;
    println!("{}", output::title("Package rename"));
    if outcome == RenameOutcome::Unchanged {
        output::status_line(Status::Skip, &old_name, "old and new names are identical");
    } else {
        println!(
            "{}",
            output::success(format!(
                "Package '{}' renamed to '{}'.",
                old_name.trim(),
                new_name.trim()
            ))
        );
    }

    Ok(())
}

pub async fn run_add_entry(name: String, paths: &UpstreamPaths) -> Result<()> {
    package_op::add_entry(&name, paths).await?;
    println!("{}", output::title("Package add-entry"));
    output::status_line(Status::Ok, &name, "entry added");
    Ok(())
}

pub async fn run_rm_entry(name: String, paths: &UpstreamPaths) -> Result<()> {
    package_op::remove_entry(&name, paths)?;
    println!("{}", output::title("Package rm-entry"));
    output::status_line(Status::Ok, &name, "entry removed");
    Ok(())
}
