use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use console::style;
use std::path::Path;

use crate::{
    models::upstream::{Package, PackageExecutable},
    providers::assets::filenames::simplify::executable_alias,
    services::{
        artifact::{
            archive_layout, compression_handler, executable_candidates, permission_handler,
        },
        integration::{CompletionManager, UnitManager},
        packaging::staging::InstallWorkspace,
    },
    utils::filesystem::safe_move,
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

fn set_executable_aliases(
    package: &mut Package,
    paths: Vec<std::path::PathBuf>,
    primary_alias: Option<String>,
) {
    package.executables = paths
        .into_iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let filename = path.file_name()?.to_string_lossy();
            let name = if index == 0 {
                primary_alias.clone().unwrap_or_else(|| {
                    executable_alias(&filename, package.match_pattern.as_slice()).to_lowercase()
                })
            } else {
                executable_alias(&filename, package.match_pattern.as_slice()).to_lowercase()
            };

            Some(PackageExecutable { path, name })
        })
        .collect();
}

/// Stage enabled integrations from a package root. Discovery failures are
/// reported without failing an otherwise-successful package installation.
pub(super) fn stage_integrations_from_root<H>(
    workspace: &InstallWorkspace,
    executable_name: &str,
    root: &Path,
    message_callback: &mut Option<H>,
) where
    H: FnMut(&str),
{
    if workspace.integration.systemd_units
        && let Err(err) = UnitManager::stage_from_root(root, &workspace.units_dir)
    {
        message!(message_callback, "User unit discovery skipped: {err:#}");
    }

    if workspace.integration.shell_completions
        && let Err(err) = CompletionManager::with_paths(workspace.completions.clone())
            .install_from_root(executable_name, root, message_callback)
    {
        message!(
            message_callback,
            "{}",
            style(format!("Completion install skipped: {err}")).yellow()
        );
    }
}

pub(super) fn handle_archive<H>(
    workspace: &InstallWorkspace,
    asset_path: &Path,
    extract_cache: &Path,
    mut package: Package,
    message_callback: &mut Option<H>,
    extraction_progress: &mut dyn FnMut(u64, u64),
) -> Result<Package>
where
    H: FnMut(&str),
{
    let filename = asset_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid archive path: no filename"))?
        .to_string_lossy()
        .to_string();

    message!(message_callback, "Extracting directory '{filename}' ...");

    let extracted_path = compression_handler::decompress_with_progress(
        asset_path,
        extract_cache,
        extraction_progress,
    )
    .context(format!("Failed to extract archive '{}'", filename))?;

    if extracted_path.is_file() {
        return handle_file(workspace, &extracted_path, package, message_callback);
    }

    if workspace.integration.systemd_units
        && let Err(err) = UnitManager::stage_from_root(&extracted_path, &workspace.units_dir)
    {
        message!(message_callback, "User unit discovery skipped: {err:#}");
    }

    let dirname = extracted_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = workspace.archives_dir.join(dirname);
    let install_root = archive_layout::select_nested_archive_root(
        &extracted_path,
        &package,
        &workspace.previous_executables,
    )
    .unwrap_or_else(|| extracted_path.clone());

    message!(
        message_callback,
        "Moving directory to '{}' ...",
        out_path.display()
    );

    safe_move::move_file_or_dir(&install_root, &out_path).context(format!(
        "Failed to move extracted directory from '{}' to '{}'",
        install_root.display(),
        out_path.display()
    ))?;

    message!(message_callback, "Searching for executable ...");

    let preferred_name = package.repo_slug.rsplit('/').next().unwrap_or_default();
    let executable_paths =
        discover_archive_executables(&out_path, preferred_name, &workspace.previous_executables)?;
    if executable_paths.is_empty() {
        anyhow::bail!(
            "Archive '{}' contains no executable file in its root or bin directory",
            out_path.display()
        );
    }

    for exec_path in &executable_paths {
        permission_handler::make_executable(exec_path).context(format!(
            "Failed to make '{}' executable",
            exec_path.display()
        ))?;
    }

    message!(
        message_callback,
        "Added executable permission for '{}'",
        executable_paths[0]
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| executable_paths[0].display().to_string())
    );

    set_executable_aliases(&mut package, executable_paths, None);
    stage_integrations_from_root(
        workspace,
        package.primary_executable_name(),
        &out_path,
        message_callback,
    );
    package.install_path = Some(out_path);
    package.last_upgraded = Utc::now();
    Ok(package)
}

fn discover_archive_executables(
    root: &Path,
    preferred_name: &str,
    previous_executables: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>> {
    discover_archive_executables_with(root, preferred_name, previous_executables, |candidates| {
        select_executable_candidate(root, candidates)
    })
}

fn discover_archive_executables_with(
    root: &Path,
    preferred_name: &str,
    previous_executables: &[std::path::PathBuf],
    choose: impl FnOnce(&[executable_candidates::ExecutableCandidate]) -> Result<std::path::PathBuf>,
) -> Result<Vec<std::path::PathBuf>> {
    let executables = executable_candidates::find_executables(root, preferred_name);
    if !executables.is_empty() {
        return Ok(executables);
    }

    if !previous_executables.is_empty() {
        let reused = previous_executables
            .iter()
            .map(|path| root.join(path))
            .collect::<Vec<_>>();
        if reused.iter().all(|path| path.is_file()) {
            return Ok(reused);
        }
    }
    let candidates = executable_candidates::find_executable_candidates(root, preferred_name);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![choose(&candidates)?])
}

fn select_executable_candidate(
    root: &Path,
    candidates: &[executable_candidates::ExecutableCandidate],
) -> Result<std::path::PathBuf> {
    let labels = candidates
        .iter()
        .map(|candidate| {
            format!(
                "{} (score: {})",
                candidate
                    .path
                    .strip_prefix(root)
                    .unwrap_or(&candidate.path)
                    .display(),
                candidate.score
            )
        })
        .collect::<Vec<_>>();
    if crate::output::assume_yes() {
        if candidates.len() != 1 {
            anyhow::bail!(
                "Executable discovery found multiple possible commands in '{}': {}. Re-run interactively to select one; --yes cannot choose between candidates.",
                root.display(),
                labels.join(", ")
            );
        }
        return Ok(candidates[0].path.clone());
    }
    let index = crate::output::select_from_list("Could not find executable. Candidates:", &labels)?
        .ok_or_else(|| anyhow!("Cancelled executable selection"))?;
    Ok(candidates[index].path.clone())
}

pub(super) fn handle_compressed<H>(
    workspace: &InstallWorkspace,
    asset_path: &Path,
    extract_cache: &Path,
    package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    let filename = asset_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid compressed path: no filename"))?
        .to_string_lossy()
        .to_string();

    message!(message_callback, "Extracting file '{}' ...", filename);

    let extracted_path = compression_handler::decompress(asset_path, extract_cache)
        .context(format!("Failed to decompress '{}'", filename))?;

    handle_file(workspace, &extracted_path, package, message_callback)
}

#[cfg(target_os = "linux")]
pub(super) async fn handle_appimage<H>(
    workspace: &InstallWorkspace,
    asset_path: &Path,
    mut package: Package,
    add_entry: bool,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    let filename = asset_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = workspace.appimages_dir.join(filename);

    message!(
        message_callback,
        "Moving file to '{}' ...",
        out_path.display()
    );

    safe_move::move_file_or_dir(asset_path, &out_path).context(format!(
        "Failed to move AppImage to '{}'",
        out_path.display()
    ))?;

    permission_handler::make_executable(&out_path).context(format!(
        "Failed to make AppImage '{}' executable",
        filename.to_string_lossy()
    ))?;

    message!(message_callback, "Made '{}' executable", filename.display());

    let extractor = if workspace.integration.systemd_units
        || workspace.integration.shell_completions
        || add_entry
    {
        match crate::services::artifact::AppImageExtractor::new() {
            Ok(extractor) => Some(extractor),
            Err(err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!("AppImage integration scan skipped: {err}")).yellow()
                );

                None
            }
        }
    } else {
        None
    };

    let integration_root = if let Some(extractor) = &extractor {
        match extractor
            .extract(&package.id, &out_path, message_callback)
            .await
        {
            Ok(root) => Some(root),
            Err(err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!("AppImage integration scan skipped: {err}")).yellow()
                );

                None
            }
        }
    } else {
        None
    };

    let primary_alias = integration_root.as_deref().and_then(|root| {
        add_entry
            .then(|| appimage_try_exec(root, &package.id))
            .flatten()
    });

    package.install_path = Some(out_path.clone());
    set_executable_aliases(&mut package, vec![out_path], primary_alias);
    if let Some(root) = integration_root {
        stage_integrations_from_root(
            workspace,
            package.primary_executable_name(),
            &root,
            message_callback,
        );
    }

    package.last_upgraded = Utc::now();
    Ok(package)
}

pub(super) fn handle_file<H>(
    workspace: &InstallWorkspace,
    asset_path: &Path,
    mut package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    let filename = asset_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = workspace.binaries_dir.join(filename);

    message!(
        message_callback,
        "Moving file to '{}' ...",
        out_path.display()
    );

    safe_move::move_file_or_dir(asset_path, &out_path)
        .context(format!("Failed to move binary to '{}'", out_path.display()))?;

    permission_handler::make_executable(&out_path).context(format!(
        "Failed to make binary '{}' executable",
        filename.to_string_lossy()
    ))?;

    message!(message_callback, "Made '{}' executable", filename.display());

    package.install_path = Some(out_path.clone());
    set_executable_aliases(&mut package, vec![out_path], None);
    package.last_upgraded = Utc::now();
    Ok(package)
}

#[cfg(target_os = "linux")]
fn appimage_try_exec(root: &Path, name: &str) -> Option<String> {
    let mut candidates = vec![
        root.join(format!("{name}.desktop")),
        root.join(format!("usr/share/applications/{name}.desktop")),
    ];

    if let Ok(entries) = glob::glob(&format!("{}/**/*.desktop", root.display())) {
        candidates.extend(entries.flatten());
    }

    candidates.sort_by_key(|path| {
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("");

        (!stem.eq_ignore_ascii_case(name), path.clone())
    });

    for path in candidates {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        let mut in_entry = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_entry = trimmed.eq_ignore_ascii_case("[Desktop Entry]");
                continue;
            }

            if in_entry && let Some(value) = trimmed.strip_prefix("TryExec=") {
                let value = value.trim();
                if !value.is_empty() && !value.contains('/') {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::{appimage_try_exec, set_executable_aliases};
    use crate::models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    };
    use std::{fs, path::PathBuf};

    fn package() -> Package {
        Package::with_defaults(
            "github:owner/WezTerm".to_string(),
            "owner/WezTerm".to_string(),
            Filetype::AppImage,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn executable_aliases_are_lowercase_by_default() {
        let mut package = package();
        set_executable_aliases(&mut package, vec![PathBuf::from("/tmp/WezTerm")], None);

        assert_eq!(package.primary_executable_name(), "wezterm");
    }

    #[test]
    fn embedded_try_exec_remains_the_primary_alias() {
        let mut package = package();
        set_executable_aliases(
            &mut package,
            vec![PathBuf::from("/tmp/WezTerm")],
            Some("wezterm".to_string()),
        );

        assert_eq!(package.primary_executable_name(), "wezterm");
    }

    #[cfg(unix)]
    #[test]
    fn archive_recovery_requires_acceptance_before_permissions_change() {
        use crate::{
            services::artifact::{compression_handler, executable_candidates, permission_handler},
            utils::test_support,
        };
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        use zip::{ZipWriter, write::SimpleFileOptions};

        let root = test_support::temp_root("upstream-archive-recovery", "confirmation");
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("tool.zip");
        let mut zip = ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("tool", SimpleFileOptions::default().unix_permissions(0o644))
            .unwrap();
        zip.write_all(b"#!/bin/sh\necho hello\n").unwrap();
        zip.finish().unwrap();
        let extracted = compression_handler::decompress(&archive, &root.join("extracted")).unwrap();
        let command = extracted.join("tool");
        let declined =
            super::discover_archive_executables_with(&extracted, "tool", &[], |candidates| {
                assert_eq!(candidates.len(), 1);
                assert_eq!(candidates[0].path, command);
                assert_eq!(candidates[0].score, 70);
                Err(anyhow::anyhow!("Cancelled"))
            });
        assert!(declined.is_err());
        assert_eq!(
            fs::metadata(&command).unwrap().permissions().mode() & 0o111,
            0
        );

        let accepted =
            super::discover_archive_executables_with(&extracted, "tool", &[], |candidates| {
                Ok(candidates[0].path.clone())
            })
            .unwrap();
        assert_eq!(accepted, vec![command.clone()]);
        for path in &accepted {
            permission_handler::make_executable(path).unwrap();
        }
        assert_eq!(
            executable_candidates::find_executables(&extracted, "tool"),
            accepted
        );
        // Normal discovery must not ask again once executable permissions exist.
        assert_eq!(
            super::discover_archive_executables_with(&extracted, "tool", &[], |_| {
                panic!("normal discovery must not prompt")
            })
            .unwrap(),
            vec![command]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archives_without_plausible_commands_do_not_prompt() {
        let root = crate::utils::test_support::temp_root("upstream-archive-recovery", "no-command");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), b"Documentation only").unwrap();
        assert!(
            super::discover_archive_executables_with(&root, "tool", &[], |_| {
                panic!("documentation must not be proposed as a command")
            })
            .unwrap()
            .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_upgrade_reuses_persisted_relative_path_without_checking_contents() {
        use crate::{
            models::upstream::PackageExecutable, services::packaging::staging::InstallWorkspace,
            utils::test_support,
        };
        use std::{io::Write, os::unix::fs::PermissionsExt};
        use zip::{ZipWriter, write::SimpleFileOptions};

        let root = test_support::temp_root("upstream-archive-reuse", "persisted");
        fs::create_dir_all(&root).unwrap();
        let mut previous = package();
        previous.filetype = Filetype::Archive;
        let old_install = root.join("packages/archives/old-version");
        previous.install_path = Some(old_install.clone());
        previous.executables = vec![PackageExecutable {
            path: old_install.join("bin/bw"),
            name: "bw".to_string(),
        }];
        let saved = serde_json::to_string(&previous).unwrap();
        let previous: Package = serde_json::from_str(&saved).unwrap();
        let paths = test_support::upstream_paths(&root);
        let workspace = InstallWorkspace::new(&paths, &previous.id)
            .unwrap()
            .reuse_executables_from(&previous);
        assert_eq!(
            workspace.previous_executables,
            vec![PathBuf::from("bin/bw")]
        );
        let mut replacement = previous;
        // Match the upgrader's fresh replacement metadata.
        replacement.install_path = None;
        replacement.executables.clear();

        let archive = root.join("new-version.zip");
        let mut zip = ZipWriter::new(fs::File::create(&archive).unwrap());
        let options = SimpleFileOptions::default().unix_permissions(0o644);
        zip.start_file("bin/bw", options).unwrap();
        // Deliberately unrecognizable: reuse must check existence only.
        zip.write_all(b"completely changed binary contents")
            .unwrap();
        zip.start_file("README", options).unwrap();
        zip.write_all(b"Keep the bin directory inside the archive root")
            .unwrap();
        zip.finish().unwrap();

        let installed = super::handle_archive(
            &workspace,
            &archive,
            &root.join("extract"),
            replacement,
            &mut None::<fn(&str)>,
            &mut |_, _| {},
        )
        .unwrap();
        let command = workspace.archives_dir.join("new-version/bin/bw");
        assert_eq!(installed.executables[0].path, command);
        assert_eq!(
            fs::read(&command).unwrap(),
            b"completely changed binary contents"
        );
        assert_ne!(
            fs::metadata(&command).unwrap().permissions().mode() & 0o111,
            0
        );
        drop(workspace);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_recorded_command_returns_to_candidate_selection() {
        use std::os::unix::fs::PermissionsExt;
        let root = crate::utils::test_support::temp_root("upstream-archive-reuse", "missing");
        fs::create_dir_all(&root).unwrap();
        let command = root.join("new-command");
        fs::write(&command, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o644)).unwrap();
        let mut prompted = false;
        let found = super::discover_archive_executables_with(
            &root,
            "tool",
            &[PathBuf::from("old-command")],
            |candidates| {
                prompted = true;
                assert_eq!(candidates.len(), 1);
                Ok(candidates[0].path.clone())
            },
        )
        .unwrap();
        assert!(prompted);
        assert_eq!(found, vec![command]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normal_discovery_takes_precedence_over_recorded_command() {
        use std::os::unix::fs::PermissionsExt;
        let root = crate::utils::test_support::temp_root("upstream-archive-reuse", "normal-first");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old-command"), b"old choice still exists").unwrap();
        fs::set_permissions(root.join("old-command"), fs::Permissions::from_mode(0o644)).unwrap();
        let command = root.join("new-command");
        fs::write(&command, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
        let found = super::discover_archive_executables_with(
            &root,
            "tool",
            &[PathBuf::from("old-command")],
            |_| panic!("normal discovery should not prompt"),
        )
        .unwrap();
        assert_eq!(found, vec![command]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn appimage_try_exec_is_used_as_primary_alias() {
        let root = std::env::temp_dir().join(format!(
            "upstream-appimage-desktop-test-{}",
            std::process::id()
        ));

        fs::create_dir_all(root.join("usr/share/applications")).expect("create root");
        fs::write(
            root.join("usr/share/applications/WezTerm.desktop"),
            "[Desktop Entry]\nName=WezTerm\nTryExec=wezterm\n",
        )
        .expect("write desktop entry");

        assert_eq!(
            appimage_try_exec(&root, "WezTerm"),
            Some("wezterm".to_string())
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn integration_discovery_respects_each_setting_independently() {
        use crate::{
            services::{
                integration::{CompletionManager, CompletionShell},
                packaging::staging::InstallWorkspace,
            },
            utils::test_support,
        };
        for (units, completions) in [(false, false), (true, false), (false, true), (true, true)] {
            let root = test_support::temp_root(
                "upstream-integration-settings",
                &format!("{units}-{completions}"),
            );

            let paths = test_support::upstream_paths(&root);
            fs::create_dir_all(paths.config.config_file.parent().unwrap()).unwrap();
            fs::write(
                &paths.config.config_file,
                format!(
                    "[integration]\nsystemd_units = {units}\nshell_completions = {completions}\n"
                ),
            )
            .unwrap();
            let source = root.join("source");
            fs::create_dir_all(source.join("lib/systemd/user")).unwrap();
            fs::create_dir_all(source.join("completions")).unwrap();
            fs::write(
                source.join("lib/systemd/user/tool.service"),
                "[Service]\nExecStart=/usr/bin/tool\n",
            )
            .unwrap();
            fs::write(
                source.join("completions/tool.bash"),
                "complete -W hello tool\n",
            )
            .unwrap();
            let workspace = InstallWorkspace::new(&paths, "tool").unwrap();
            super::stage_integrations_from_root(&workspace, "tool", &source, &mut None::<fn(&str)>);
            assert_eq!(workspace.units_dir.join("tool.service").exists(), units);
            let has_bash = CompletionManager::installed_shells().contains(&CompletionShell::Bash);
            assert_eq!(
                workspace.completions.bash_dir.join("tool").exists(),
                completions && has_bash
            );

            drop(workspace);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
