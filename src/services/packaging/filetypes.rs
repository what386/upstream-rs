use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use console::style;
use std::path::Path;

use crate::{
    models::upstream::{Package, PackageExecutable},
    services::{
        artifact::{archive_layout, compression_handler, permission_handler},
        integration::CompletionManager,
        packaging::staging::InstallWorkspace,
    },
    utils::{filenames::simplify::executable_alias, filesystem::safe_move},
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

/// Best-effort completion install for a package root; failures are reported
/// through `message_callback` rather than propagated, since missing shell
/// completions should never fail an otherwise-successful install.
pub(super) fn install_completions_from_root<H>(
    workspace: &InstallWorkspace,
    package_name: &str,
    root: &Path,
    message_callback: &mut Option<H>,
) where
    H: FnMut(&str),
{
    if let Err(err) = CompletionManager::with_paths(workspace.completions.clone())
        .install_from_root(package_name, root, message_callback)
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

    let dirname = extracted_path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = workspace.archives_dir.join(dirname);
    let install_root = archive_layout::select_nested_archive_root(&extracted_path, &package)
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
    let executable_paths = permission_handler::find_executables(&out_path, preferred_name);
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

    install_completions_from_root(workspace, &package.id, &out_path, message_callback);
    set_executable_aliases(&mut package, executable_paths, None);
    package.install_path = Some(out_path);
    package.last_upgraded = Utc::now();
    Ok(package)
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

    let completion_root = match crate::services::artifact::AppImageExtractor::new() {
        Ok(extractor) => match extractor
            .extract(&package.id, &out_path, message_callback)
            .await
        {
            Ok(root) => Some(root),
            Err(err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!("AppImage completion scan skipped: {err}")).yellow()
                );

                None
            }
        },
        Err(err) => {
            message!(
                message_callback,
                "{}",
                style(format!("AppImage completion scan skipped: {err}")).yellow()
            );

            None
        }
    };

    let primary_alias = completion_root.as_deref().and_then(|root| {
        add_entry
            .then(|| appimage_try_exec(root, &package.id))
            .flatten()
    });

    if let Some(root) = completion_root {
        install_completions_from_root(workspace, &package.id, &root, message_callback);
    }

    package.install_path = Some(out_path.clone());
    set_executable_aliases(&mut package, vec![out_path], primary_alias);
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
}
