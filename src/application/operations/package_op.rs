use crate::{
    application::cli::arguments::PackageSettingKey,
    models::{common::enums::TrustMode, upstream::Package},
    providers::pattern_matcher::PatternTable,
    services::integration::{DesktopManager, LauncherManager},
    storage::database::{PackageDatabase, PackageSettings},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use std::collections::HashSet;

fn load_package(database: &PackageDatabase, name: &str) -> Result<Package> {
    database
        .get_package(name)?
        .ok_or_else(|| anyhow!("Package '{name}' not found"))
}

pub fn get(name: &str, paths: &UpstreamPaths) -> Result<(Package, PackageSettings)> {
    let database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let package = load_package(&database, name)?;
    let settings = database
        .get_package_settings(&package.id)?
        .unwrap_or_else(|| PackageSettings::new(&package.id));
    Ok((package, settings))
}

pub fn set(name: &str, assignments: Vec<String>, paths: &UpstreamPaths) -> Result<()> {
    let mut package_database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let mut package = load_package(&package_database, name)?;
    let mut settings = package_database
        .get_package_settings(&package.id)?
        .unwrap_or_else(|| PackageSettings::new(&package.id));
    let previous = settings.clone();
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
            PackageSettingKey::DesktopLaunchOptions => {
                if cfg!(target_os = "macos") {
                    bail!("Desktop integration is unsupported on macOS");
                }
                LauncherManager::validate(value)?;
                settings.desktop_launch_options = Some(value.to_string());
            }
            PackageSettingKey::TrustMode => {
                settings.trust_mode = Some(
                    TrustMode::from_str(value.trim(), true)
                        .map_err(|err| anyhow!("Invalid trust_mode: {err}"))?,
                )
            }
        }
    }

    commit(&mut package_database, &package, &previous, &settings, paths)
}

pub fn unset(name: &str, keys: Vec<PackageSettingKey>, paths: &UpstreamPaths) -> Result<()> {
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let mut package = load_package(&database, name)?;
    let mut settings = database
        .get_package_settings(&package.id)?
        .unwrap_or_else(|| PackageSettings::new(&package.id));
    let previous = settings.clone();
    for key in selected_setting_keys(keys) {
        match key {
            PackageSettingKey::MatchPattern => package.match_pattern = PatternTable::empty(),
            PackageSettingKey::ExcludePattern => package.exclude_pattern = PatternTable::empty(),
            PackageSettingKey::TrustMode => settings.trust_mode = None,
            PackageSettingKey::DesktopLaunchOptions => {
                if cfg!(target_os = "macos") {
                    bail!("Desktop integration is unsupported on macOS");
                }
                settings.desktop_launch_options = None;
            }
        }
    }
    commit(&mut database, &package, &previous, &settings, paths)
}

fn commit(
    database: &mut PackageDatabase,
    package: &Package,
    previous: &PackageSettings,
    settings: &PackageSettings,
    paths: &UpstreamPaths,
) -> Result<()> {
    if previous.desktop_launch_options == settings.desktop_launch_options {
        return database.upsert_package_with_settings(package, settings);
    }
    let snapshot = DesktopManager::snapshot_for_package(paths, package)?;
    let result = (|| {
        DesktopManager::update_launch_targets(paths, package, settings)?;
        database.upsert_package_with_settings(package, settings)
    })();
    if let Err(error) = result {
        snapshot
            .restore(Some(package))
            .context(format!("Desktop launch options update failed: {error:#}"))?;
        return Err(error).context("Desktop launch options update failed; previous entry restored");
    }
    Ok(())
}

pub fn pin(name: &str, pinned: bool, paths: &UpstreamPaths) -> Result<()> {
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let mut manager = super::metadata_op::MetadataManager::new(&mut database);
    if pinned {
        manager.pin_package(name)
    } else {
        manager.unpin_package(name)
    }
}

pub fn rename(
    old_name: &str,
    new_name: &str,
    paths: &UpstreamPaths,
) -> Result<super::rename_op::RenameOutcome> {
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    super::rename_op::rename_alias(&mut database, paths, old_name.trim(), new_name.trim())
}

pub async fn add_entry(name: &str, paths: &UpstreamPaths) -> Result<()> {
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let mut package = load_package(&database, name)?;
    let manager = DesktopManager::new(paths)?;
    let snapshot = DesktopManager::snapshot_for_package(paths, &package)?;
    let mut ignored = Some(|_: &str| {});
    let result = async {
        manager
            .enable_package_entry(&mut package, &mut ignored)
            .await?;
        database.upsert_package(&package)
    }
    .await;
    if let Err(error) = result {
        snapshot.restore(Some(&package))?;
        return Err(error);
    }
    Ok(())
}

pub fn remove_entry(name: &str, paths: &UpstreamPaths) -> Result<()> {
    let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)?;
    let mut package = load_package(&database, name)?;
    let snapshot = DesktopManager::snapshot_for_package(paths, &package)?;
    let mut ignored = Some(|_: &str| {});
    let result = (|| {
        DesktopManager::new(paths)?.disable_package_entry(&mut package, &mut ignored)?;
        database.upsert_package(&package)
    })();
    if let Err(error) = result {
        snapshot.restore(Some(&package))?;
        return Err(error);
    }
    Ok(())
}

fn parse_setting_key(raw: &str) -> Result<PackageSettingKey> {
    match raw.trim().replace('-', "_").as_str() {
        "match_pattern" => Ok(PackageSettingKey::MatchPattern),
        "exclude_pattern" => Ok(PackageSettingKey::ExcludePattern),
        "trust_mode" => Ok(PackageSettingKey::TrustMode),
        "desktop_launch_options" => Ok(PackageSettingKey::DesktopLaunchOptions),
        other => bail!(
            "Unknown package setting '{}'; supported settings: match_pattern, exclude_pattern, trust_mode, desktop_launch_options",
            other
        ),
    }
}

fn setting_name(key: PackageSettingKey) -> &'static str {
    match key {
        PackageSettingKey::MatchPattern => "match_pattern",
        PackageSettingKey::ExcludePattern => "exclude_pattern",
        PackageSettingKey::TrustMode => "trust_mode",
        PackageSettingKey::DesktopLaunchOptions => "desktop_launch_options",
    }
}

pub fn selected_setting_keys(keys: Vec<PackageSettingKey>) -> Vec<PackageSettingKey> {
    let keys = if keys.is_empty() {
        vec![
            PackageSettingKey::MatchPattern,
            PackageSettingKey::ExcludePattern,
            PackageSettingKey::TrustMode,
            PackageSettingKey::DesktopLaunchOptions,
        ]
    } else {
        keys
    };

    let mut seen = HashSet::new();
    keys.into_iter().filter(|key| seen.insert(*key)).collect()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        models::{
            common::{
                DesktopEntry,
                enums::{Channel, Filetype, Provider},
            },
            upstream::PackageExecutable,
        },
        services::integration::SymlinkManager,
        utils::test_support,
    };
    use std::fs;

    #[tokio::test]
    async fn desktop_options_are_reversible_persisted_and_leave_terminal_links_untouched() {
        let root = test_support::temp_root("desktop-options", "lifecycle");
        let paths = test_support::upstream_paths(&root);
        let mut package = Package::with_defaults(
            "tool".into(),
            "owner/tool".into(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let binary = root.join("my apps/tool");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, "binary").unwrap();
        package.install_path = Some(binary.clone());
        package.executables.push(PackageExecutable {
            name: "tool".into(),
            path: binary.clone(),
        });
        let mut db = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        db.upsert_package(&package).unwrap();
        fs::create_dir_all(&paths.state.symlinks_dir).unwrap();
        SymlinkManager::new(&paths.state.symlinks_dir)
            .add_link(&binary, "tool")
            .unwrap();
        let entry = DesktopManager::managed_entry_path(&paths, &package.id).unwrap();
        let base = format!(
            "[Desktop Entry]\nName=My Tool\nIcon=tool\nDBusActivatable=true\nExec={} %U\n\n[Desktop Action New]\nName=New Window\nExec={} --new-window %u\n",
            DesktopEntry::quote_exec_path(&binary),
            DesktopEntry::quote_exec_path(&binary)
        );
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, &base).unwrap();
        let apply = |template: &str| {
            set(
                "tool",
                vec![format!("desktop_launch_options={template}")],
                &paths,
            )
        };
        apply("env FOO=bar {} --profile 'Work Profile'").unwrap();
        let content = fs::read_to_string(&entry).unwrap();
        assert!(content.contains(&format!(
            "Exec=env FOO=bar {} --profile \"Work Profile\" %U",
            DesktopEntry::quote_exec_path(&binary)
        )));
        assert!(content.contains("--profile \"Work Profile\" --new-window %u"));
        assert!(content.contains("DBusActivatable=false"));
        assert!(content.contains("Icon=tool\n"));
        assert_eq!(
            fs::read_link(paths.state.symlinks_dir.join("tool")).unwrap(),
            binary
        );
        assert!(!paths.dirs.generated_dir.join("wrappers").exists());
        assert_eq!(
            get("tool", &paths)
                .unwrap()
                .1
                .desktop_launch_options
                .as_deref(),
            Some("env FOO=bar {} --profile 'Work Profile'")
        );
        apply("{} --disable-gpu").unwrap();
        let updated = fs::read_to_string(&entry).unwrap();
        assert!(!updated.contains("FOO=bar"));
        assert!(!updated.contains("Work Profile"));
        assert!(apply("{} {}").is_err());
        assert_eq!(fs::read_to_string(&entry).unwrap(), updated);
        unset(
            "tool",
            vec![PackageSettingKey::DesktopLaunchOptions],
            &paths,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&entry).unwrap(), base);
        assert!(
            get("tool", &paths)
                .unwrap()
                .1
                .desktop_launch_options
                .is_none()
        );

        // Save before add-entry, then exercise the creation path used by desktop integration.
        fs::remove_file(&entry).unwrap();
        apply("{} --disable-gpu").unwrap();
        assert!(!entry.exists());
        add_entry("tool", &paths).await.unwrap();
        let created = fs::read_to_string(&entry).unwrap();
        assert!(created.contains(" --disable-gpu\n"));
        // Recreating an entry must read persisted options without duplicating them.
        add_entry("tool", &paths).await.unwrap();
        assert_eq!(fs::read_to_string(&entry).unwrap(), created);
        drop(db);
        fs::remove_dir_all(root).unwrap();
    }
}
