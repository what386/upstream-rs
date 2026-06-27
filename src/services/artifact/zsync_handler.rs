use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, anyhow};
use tokio::process::Command;

use crate::{
    models::{
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub fn is_asset(asset_name: &str, target_asset_name: &str) -> bool {
    let Some(asset_file_name) = Path::new(asset_name).file_name().and_then(|name| name.to_str())
    else {
        return false;
    };
    let Some(target_file_name) = Path::new(target_asset_name)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return false;
    };

    asset_file_name.eq_ignore_ascii_case(&format!("{target_file_name}.zsync"))
}

pub fn find_asset<'a>(release: &'a Release, target_asset: &Asset) -> Option<&'a Asset> {
    release
        .assets
        .iter()
        .find(|asset| is_asset(&asset.name, &target_asset.name))
}

pub async fn update_selected_asset<H>(
    package: &Package,
    release: &Release,
    target_asset: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    target_path: &Path,
    message_callback: Option<&mut H>,
) -> Result<bool>
where
    H: FnMut(&str),
{
    let Some(zsync_asset) = find_asset(release, target_asset) else {
        return Ok(false);
    };

    update_asset(
        package,
        zsync_asset,
        provider_manager,
        download_cache,
        target_path,
        message_callback,
    )
    .await?;

    Ok(true)
}

pub async fn update_asset<H>(
    package: &Package,
    zsync_asset: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    target_path: &Path,
    mut message_callback: Option<&mut H>,
) -> Result<()>
where
    H: FnMut(&str),
{
    ensure_target_file(target_path)?;
    ensure_zsync_available().await?;

    message!(
        message_callback,
        "Downloading zsync descriptor '{}'",
        zsync_asset.name
    );

    let mut no_progress: Option<fn(u64, u64)> = None;
    let zsync_path = provider_manager
        .download_asset(
            zsync_asset,
            &package.provider,
            download_cache,
            &mut no_progress,
        )
        .await
        .with_context(|| format!("Failed to download zsync descriptor '{}'", zsync_asset.name))?;

    let output_path = zsync_output_path(target_path);
    if output_path.exists() {
        let _ = fs::remove_file(&output_path);
    }

    message!(
        message_callback,
        "Updating '{}' with '{}'",
        target_path.display(),
        zsync_asset.name
    );

    let result = run_zsync_update(Path::new("zsync"), target_path, &output_path, &zsync_path).await;
    if result.is_err() {
        let _ = fs::remove_file(&output_path);
    }
    result?;

    if !output_path.is_file() {
        return Err(anyhow!(
            "zsync completed but output file was not created at '{}'",
            output_path.display()
        ));
    }

    fs::rename(&output_path, target_path).with_context(|| {
        format!(
            "Failed to replace '{}' with zsync output '{}'",
            target_path.display(),
            output_path.display()
        )
    })?;

    message!(
        message_callback,
        "Updated '{}' via zsync",
        target_path.display()
    );

    Ok(())
}

fn ensure_target_file(target_path: &Path) -> Result<()> {
    if !target_path.exists() {
        return Err(anyhow!(
            "Target file for zsync update was not found: '{}'",
            target_path.display()
        ));
    }

    if !target_path.is_file() {
        return Err(anyhow!(
            "Target path for zsync update is not a file: '{}'",
            target_path.display()
        ));
    }

    Ok(())
}

async fn ensure_zsync_available() -> Result<()> {
    ensure_zsync_available_at(Path::new("zsync")).await
}

async fn ensure_zsync_available_at(command: &Path) -> Result<()> {
    let status = spawn_zsync_command(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(zsync_spawn_error)?;

    if !status.success() {
        return Err(anyhow!(
            "Required external binary 'zsync' is not executable or returned a failing status"
        ));
    }

    Ok(())
}

async fn run_zsync_update(
    command: &Path,
    target_path: &Path,
    output_path: &Path,
    zsync_path: &Path,
) -> Result<()> {
    let output = spawn_zsync_command(command)
        .arg("-i")
        .arg(target_path)
        .arg("-o")
        .arg(output_path)
        .arg(zsync_path)
        .output()
        .await
        .map_err(zsync_spawn_error)?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("status {}", output.status)
    };

    Err(anyhow!("zsync update failed: {detail}"))
}

fn spawn_zsync_command(command: &Path) -> Command {
    Command::new(command)
}

fn zsync_output_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    target_path.with_file_name(format!("{file_name}.zsync-update"))
}

fn zsync_spawn_error(error: io::Error) -> anyhow::Error {
    match error.kind() {
        io::ErrorKind::NotFound => {
            anyhow!("Required external binary 'zsync' was not found in PATH")
        }
        io::ErrorKind::PermissionDenied => {
            anyhow!("Required external binary 'zsync' is not executable")
        }
        _ => anyhow!("Failed to execute 'zsync': {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_zsync_available_at, find_asset, is_asset, run_zsync_update, zsync_output_path,
    };
    use crate::{
        models::{
            common::Version,
            provider::{Asset, Release},
        },
    };
    use chrono::{TimeZone, Utc};
    use std::{fs, path::Path};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("upstream-zsync-test-{name}-{nanos}"))
    }

    fn asset(name: &str) -> Asset {
        Asset::new(
            format!("https://example.invalid/{name}"),
            1,
            name.to_string(),
            123,
            Utc.with_ymd_and_hms(2026, 6, 27, 12, 0, 0).unwrap(),
        )
    }

    #[test]
    fn zsync_sidecar_name_matches_target_asset() {
        assert!(is_asset("tool.tar.gz.zsync", "tool.tar.gz"));
        assert!(is_asset("TOOL.TAR.GZ.ZSYNC", "tool.tar.gz"));
        assert!(!is_asset("tool.zsync", "other-tool"));
    }

    #[test]
    fn finds_matching_zsync_sidecar_for_selected_asset() {
        let target = asset("tool.tar.gz");
        let release = Release {
            id: 1,
            tag: "v1.0.0".to_string(),
            name: "v1.0.0".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![asset("tool.tar.gz.zsync"), asset("other.tar.gz.zsync")],
            version: Version::new(1, 0, 0, false),
            published_at: Utc.with_ymd_and_hms(2026, 6, 27, 12, 0, 0).unwrap(),
        };

        let found = find_asset(&release, &target).expect("find zsync sidecar");
        assert_eq!(found.name, "tool.tar.gz.zsync");
    }

    #[test]
    fn zsync_output_path_uses_sibling_temp_file() {
        let output = zsync_output_path(Path::new("/tmp/tool.tar.gz"));
        assert_eq!(output, Path::new("/tmp/tool.tar.gz.zsync-update"));
    }

    #[tokio::test]
    async fn missing_zsync_binary_reports_clear_error() {
        let err = ensure_zsync_available_at(Path::new("/definitely/missing/zsync"))
            .await
            .expect_err("missing zsync");
        assert!(err.to_string().contains("not found"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn non_executable_zsync_binary_reports_clear_error() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("nonexec");
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("zsync");
        fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).expect("chmod");

        let err = ensure_zsync_available_at(&script)
            .await
            .expect_err("non executable zsync");
        assert!(err.to_string().contains("not executable"));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_zsync_update_uses_input_and_output_paths() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("run");
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("zsync");
        let input = root.join("tool.tar.gz");
        let output = root.join("tool.tar.gz.zsync-update");
        let control = root.join("tool.tar.gz.zsync");

        fs::write(&input, b"old-data").expect("write input");
        fs::write(&control, b"zsync-control").expect("write control");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 0; fi\ncp \"$2\" \"$4\"\n",
        )
        .expect("write script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");

        run_zsync_update(&script, &input, &output, &control)
            .await
            .expect("run zsync update");

        assert_eq!(fs::read(&output).expect("read output"), b"old-data");

        let _ = fs::remove_dir_all(root);
    }
}
