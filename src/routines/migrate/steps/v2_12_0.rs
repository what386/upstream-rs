use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::routines::migrate::MigrationReport;
use crate::routines::migrate::step::Step;
use crate::services::integration::ShellManager;
use crate::storage::database::PackageDatabase;
use crate::utils::platform::shells::installed_shell_commands;
use crate::utils::static_paths::UpstreamPaths;

pub struct V2_12_0;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    V2_12_0::run(paths, report)
}

impl Step for V2_12_0 {
    fn check(paths: &UpstreamPaths) -> Result<bool> {
        Ok(legacy_paths_file(paths).exists()
            || legacy_paths_nu_file(paths).exists()
            || legacy_shell_profile_hook_exists(paths)?)
    }

    fn apply(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
        let generated_dir_created = !paths.dirs.generated_dir.exists();
        fs::create_dir_all(&paths.dirs.generated_dir).with_context(|| {
            format!(
                "Failed to create generated directory '{}'",
                paths.dirs.generated_dir.display()
            )
        })?;

        let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
        ShellManager::new(&paths.config.paths_file)
            .regenerate_paths(&mut package_database, paths)
            .context("Failed to regenerate generated PATH files")?;

        rewrite_shell_profiles(paths)?;
        remove_legacy_paths_files(paths)?;

        if generated_dir_created {
            report.created_dirs += 1;
        }
        Ok(())
    }
}

fn legacy_paths_file(paths: &UpstreamPaths) -> PathBuf {
    paths.dirs.metadata_dir.join("paths.sh")
}

fn legacy_paths_nu_file(paths: &UpstreamPaths) -> PathBuf {
    paths.dirs.metadata_dir.join("paths.nu")
}

fn legacy_shell_profile_hook_exists(paths: &UpstreamPaths) -> Result<bool> {
    for shell in installed_shell_commands() {
        let Some((profile, legacy_line)) = legacy_profile_hook(&shell) else {
            continue;
        };
        let profile_path = paths.dirs.user_dir.join(profile);
        if !profile_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&profile_path).with_context(|| {
            format!("Failed to read shell profile '{}'", profile_path.display())
        })?;
        if content.contains(legacy_line) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn legacy_profile_hook(shell: &str) -> Option<(&'static str, &'static str)> {
    match shell {
        "bash" | "sh" => Some((".bashrc", legacy_source_line_bash())),
        "zsh" => Some((".zshrc", legacy_source_line_bash())),
        "fish" => Some((".config/fish/config.fish", legacy_source_line_fish())),
        "nu" => Some((".config/nushell/config.nu", legacy_source_line_nushell())),
        _ => None,
    }
}

fn legacy_source_line_bash() -> &'static str {
    "[ -f $HOME/.upstream/metadata/paths.sh ] && source $HOME/.upstream/metadata/paths.sh"
}

fn legacy_source_line_fish() -> &'static str {
    "test -f $HOME/.upstream/metadata/paths.sh; and source $HOME/.upstream/metadata/paths.sh"
}

fn legacy_source_line_nushell() -> &'static str {
    r#"const upstream_paths_nu = if ("~/.upstream/metadata/paths.nu" | path expand | path exists) { ("~/.upstream/metadata/paths.nu" | path expand) } else { null }; source-env $upstream_paths_nu"#
}

fn current_source_line_bash() -> &'static str {
    "[ -f $HOME/.upstream/generated/paths.sh ] && source $HOME/.upstream/generated/paths.sh"
}

fn current_source_line_fish() -> &'static str {
    "test -f $HOME/.upstream/generated/paths.sh; and source $HOME/.upstream/generated/paths.sh"
}

fn current_source_line_nushell() -> &'static str {
    r#"const upstream_paths_nu = if ("~/.upstream/generated/paths.nu" | path expand | path exists) { ("~/.upstream/generated/paths.nu" | path expand) } else { null }; source-env $upstream_paths_nu"#
}

fn rewrite_shell_profiles(paths: &UpstreamPaths) -> Result<()> {
    for shell in installed_shell_commands() {
        let Some((profile, old_line, new_line)) = current_profile_hook(&shell) else {
            continue;
        };

        let profile_path = paths.dirs.user_dir.join(profile);
        if !profile_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&profile_path).with_context(|| {
            format!("Failed to read shell profile '{}'", profile_path.display())
        })?;
        let updated = content.replace(old_line, new_line);
        if updated != content {
            crate::utils::filesystem::atomic_ops::write_atomic(&profile_path, updated.as_bytes())
                .with_context(|| {
                format!(
                    "Failed to update shell profile '{}'",
                    profile_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn current_profile_hook(shell: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match shell {
        "bash" | "sh" => Some((
            ".bashrc",
            legacy_source_line_bash(),
            current_source_line_bash(),
        )),
        "zsh" => Some((
            ".zshrc",
            legacy_source_line_bash(),
            current_source_line_bash(),
        )),
        "fish" => Some((
            ".config/fish/config.fish",
            legacy_source_line_fish(),
            current_source_line_fish(),
        )),
        "nu" => Some((
            ".config/nushell/config.nu",
            legacy_source_line_nushell(),
            current_source_line_nushell(),
        )),
        _ => None,
    }
}

fn remove_legacy_paths_files(paths: &UpstreamPaths) -> Result<()> {
    for path in [legacy_paths_file(paths), legacy_paths_nu_file(paths)] {
        if path.exists() {
            fs::remove_file(&path).with_context(|| {
                format!("Failed to remove legacy paths file '{}'", path.display())
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{V2_12_0, run};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::routines::migrate::step::Step;
    use crate::storage::database::PackageDatabase;
    use crate::utils::platform::shells::installed_shell_commands;
    use crate::utils::test_support;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-12-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn test_package(name: &str, install_path: PathBuf, exec_path: PathBuf) -> Package {
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path);
        package.exec_path = Some(exec_path);
        package
    }

    #[test]
    fn check_detects_legacy_paths_files() {
        let root = temp_root("check-legacy-files");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.dirs.metadata_dir.clone()).expect("create metadata");
        fs::write(paths.dirs.metadata_dir.join("paths.sh"), "legacy").expect("write paths.sh");

        assert!(V2_12_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn check_skips_current_layout_without_legacy_references() {
        let root = temp_root("check-current");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.generated_dir).expect("create generated");
        fs::write(&paths.config.paths_file, "#!/usr/bin/env sh\n").expect("write paths.sh");
        fs::write(
            &paths.config.paths_nu_file,
            "# Upstream managed PATH additions\n",
        )
        .expect("write paths.nu");

        assert!(!V2_12_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_regenerates_paths_files_and_updates_shell_profiles() {
        let root = temp_root("regen-paths");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata dir");

        let legacy_paths_file = paths.dirs.metadata_dir.join("paths.sh");
        let legacy_paths_nu_file = paths.dirs.metadata_dir.join("paths.nu");
        fs::write(&legacy_paths_file, "legacy paths").expect("write legacy paths.sh");
        fs::write(&legacy_paths_nu_file, "legacy paths").expect("write legacy paths.nu");

        let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("create package database");
        let mut package = test_package(
            "tool",
            paths.dirs.packages_dir.join("archives/tool"),
            paths.dirs.packages_dir.join("archives/tool/bin"),
        );
        package.last_upgraded = chrono::Utc::now();
        package_database
            .upsert_package(&package)
            .expect("seed package");

        for shell in installed_shell_commands() {
            let Some((profile, legacy_line)) = super::legacy_profile_hook(&shell) else {
                continue;
            };
            let profile_path = paths.dirs.user_dir.join(profile);
            if let Some(parent) = profile_path.parent() {
                fs::create_dir_all(parent).expect("create profile parent");
            }
            fs::write(&profile_path, format!("{legacy_line}\n")).expect("write profile");
        }

        let mut report = MigrationReport::default();
        run(&paths, &mut report).expect("run migration");

        assert!(paths.config.paths_file.exists());
        assert!(paths.config.paths_nu_file.exists());
        assert!(!legacy_paths_file.exists());
        assert!(!legacy_paths_nu_file.exists());
        assert!(paths.dirs.generated_dir.exists());

        let migrated_config = fs::read_to_string(&paths.config.paths_file).expect("read paths.sh");
        assert!(migrated_config.contains(&paths.state.symlinks_dir.display().to_string()));
        let migrated_nu = fs::read_to_string(&paths.config.paths_nu_file).expect("read paths.nu");
        assert!(migrated_nu.contains(&paths.state.symlinks_dir.display().to_string()));

        for shell in installed_shell_commands() {
            let Some((profile, legacy_line, new_line)) = super::current_profile_hook(&shell) else {
                continue;
            };
            let profile_path = paths.dirs.user_dir.join(profile);
            if !profile_path.exists() {
                continue;
            }
            let content = fs::read_to_string(&profile_path).expect("read shell profile");
            assert!(!content.contains(legacy_line));
            assert!(content.contains(new_line));
        }

        cleanup(&root).expect("cleanup");
    }
}
