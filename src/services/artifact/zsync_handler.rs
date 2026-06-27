use std::fs;
use std::io;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, anyhow};
use tokio::{io::AsyncReadExt, process::Command};

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
    let Some(asset_file_name) = Path::new(asset_name)
        .file_name()
        .and_then(|name| name.to_str())
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

pub async fn update_selected_asset<H, P>(
    package: &Package,
    release: &Release,
    target_asset: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    seed_path: &Path,
    output_path: &Path,
    message_callback: Option<&mut H>,
    progress_callback: &mut Option<P>,
) -> Result<bool>
where
    H: FnMut(&str),
    P: FnMut(u64, u64),
{
    let Some(zsync_asset) = find_asset(release, target_asset) else {
        return Ok(false);
    };

    update_asset(
        package,
        zsync_asset,
        provider_manager,
        download_cache,
        seed_path,
        output_path,
        target_asset.size,
        message_callback,
        progress_callback,
    )
    .await?;

    Ok(true)
}

pub async fn update_asset<H, P>(
    package: &Package,
    zsync_asset: &Asset,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    seed_path: &Path,
    output_path: &Path,
    target_size: u64,
    mut message_callback: Option<&mut H>,
    progress_callback: &mut Option<P>,
) -> Result<()>
where
    H: FnMut(&str),
    P: FnMut(u64, u64),
{
    ensure_seed_file(seed_path)?;
    let status = Command::new("zsync")
        .arg("-V")
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

    if output_path.exists() {
        let _ = fs::remove_file(&output_path);
    }

    message!(
        message_callback,
        "Updating '{}' with '{}'",
        seed_path.display(),
        zsync_asset.name
    );

    let result = run_zsync_update(
        seed_path,
        &zsync_path,
        &zsync_asset.download_url,
        output_path,
        target_size,
        progress_callback,
    )
    .await;
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

    message!(
        message_callback,
        "Updated '{}' via zsync",
        output_path.display()
    );

    Ok(())
}

fn ensure_seed_file(seed_path: &Path) -> Result<()> {
    if !seed_path.exists() {
        return Err(anyhow!(
            "Seed file for zsync update was not found: '{}'",
            seed_path.display()
        ));
    }

    if !seed_path.is_file() {
        return Err(anyhow!(
            "Seed path for zsync update is not a file: '{}'",
            seed_path.display()
        ));
    }

    Ok(())
}

async fn run_zsync_update<P>(
    seed_path: &Path,
    input_path: &Path,
    descriptor_url: &str,
    output_path: &Path,
    total_size: u64,
    progress_callback: &mut Option<P>,
) -> Result<()>
where
    P: FnMut(u64, u64),
{
    let mut child = Command::new("zsync")
        .arg(format!("-i={}", seed_path.display()))
        .arg(format!("-o={}", output_path.display()))
        .arg(format!("-u={descriptor_url}"))
        .arg(input_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(zsync_spawn_error)?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let mut stdout_done = stdout.is_none();
    let mut stderr_done = stderr.is_none();
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut stdout_chunk = [0_u8; 1024];
    let mut stderr_chunk = [0_u8; 1024];

    while !stdout_done || !stderr_done {
        tokio::select! {
            read = async {
                match stdout.as_mut() {
                    Some(stream) => stream.read(&mut stdout_chunk).await,
                    None => Ok(0),
                }
            }, if !stdout_done => {
                match read {
                    Ok(0) => stdout_done = true,
                    Ok(n) => {
                        stdout_buf.extend_from_slice(&stdout_chunk[..n]);
                        emit_zsync_progress(&stdout_buf, total_size, progress_callback);
                    }
                    Err(err) => return Err(anyhow!("Failed to read zsync stdout: {err}")),
                }
            }
            read = async {
                match stderr.as_mut() {
                    Some(stream) => stream.read(&mut stderr_chunk).await,
                    None => Ok(0),
                }
            }, if !stderr_done => {
                match read {
                    Ok(0) => stderr_done = true,
                    Ok(n) => {
                        stderr_buf.extend_from_slice(&stderr_chunk[..n]);
                        emit_zsync_progress(&stderr_buf, total_size, progress_callback);
                    }
                    Err(err) => return Err(anyhow!("Failed to read zsync stderr: {err}")),
                }
            }
        }
    }

    let status = child.wait().await.map_err(zsync_spawn_error)?;

    if status.success() {
        if let Some(cb) = progress_callback.as_mut() {
            cb(total_size, total_size);
        }
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
    let stdout = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("status {status}")
    };

    Err(anyhow!("zsync update failed: {detail}"))
}

fn emit_zsync_progress<P>(bytes: &[u8], total_size: u64, progress_callback: &mut Option<P>)
where
    P: FnMut(u64, u64),
{
    let Some(percent) = parse_zsync_percent(bytes) else {
        return;
    };
    let downloaded = ((total_size as f64) * (percent / 100.0)).round() as u64;
    if let Some(cb) = progress_callback.as_mut() {
        cb(downloaded.min(total_size), total_size);
    }
}

fn parse_zsync_percent(bytes: &[u8]) -> Option<f64> {
    let text = String::from_utf8_lossy(bytes);
    let marker = text.rfind('%')?;
    let before = &text[..marker];
    let start = before
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || ch == '.' {
                None
            } else {
                Some(idx + ch.len_utf8())
            }
        })
        .unwrap_or(0);
    let value = before[start..].trim();
    if value.is_empty() {
        return None;
    }
    value.parse::<f64>().ok().filter(|value| value.is_finite())
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
    use super::{find_asset, is_asset, parse_zsync_percent};
    use crate::models::{
        common::Version,
        provider::{Asset, Release},
    };
    use chrono::{TimeZone, Utc};

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
    fn parses_zsync_progress_percent_from_output_chunk() {
        assert_eq!(parse_zsync_percent(b"reading seed file 12.5%"), Some(12.5));
        assert_eq!(parse_zsync_percent(b"\rdownloading 100%"), Some(100.0));
        assert_eq!(parse_zsync_percent(b"no progress here"), None);
    }
}
