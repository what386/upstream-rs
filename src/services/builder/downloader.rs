use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use crate::models::{
    common::enums::{Channel, Provider},
    provider::{Asset, Release},
};
use crate::providers::provider_manager::ProviderManager;
use crate::services::integration::compression_handler;

pub struct SourceDownload {
    pub workspace_path: PathBuf,
    pub release: Release,
}

pub struct SourceDownloader<'a> {
    provider_manager: &'a ProviderManager,
    cache_dir: PathBuf,
}

impl<'a> SourceDownloader<'a> {
    pub fn new(provider_manager: &'a ProviderManager) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let cache_dir = std::env::temp_dir().join(format!("upstream-build-{nonce}"));
        std::fs::create_dir_all(&cache_dir).context(format!(
            "Failed to create build cache '{}'",
            cache_dir.display()
        ))?;

        Ok(Self {
            provider_manager,
            cache_dir,
        })
    }

    pub async fn fetch_source(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
    ) -> Result<SourceDownload> {
        let release = if let Some(tag_name) = tag {
            self.provider_manager
                .get_release_by_tag_for(repo_slug, tag_name, provider, base_url)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'",
                    tag_name, repo_slug
                ))?
        } else {
            self.provider_manager
                .get_latest_release_for(repo_slug, provider, channel, base_url)
                .await
                .context(format!(
                    "Failed to fetch latest release for '{}'",
                    repo_slug
                ))?
        };

        let primary_archive =
            self.make_source_archive_asset(repo_slug, provider, &release, base_url)?;
        let mut no_progress: Option<fn(u64, u64)> = None;
        let downloaded_primary = self
            .provider_manager
            .download_asset(
                &primary_archive,
                provider,
                &self.cache_dir,
                &mut no_progress,
            )
            .await
            .context(format!(
                "Failed to download source archive for '{}'",
                repo_slug
            ));

        let downloaded = match downloaded_primary {
            Ok(path) => path,
            Err(primary_err) => {
                if let Some(fallback) = Self::find_release_source_asset(&release) {
                    self.provider_manager
                        .download_asset(fallback, provider, &self.cache_dir, &mut no_progress)
                        .await
                        .context(format!(
                            "Failed source download for '{}' using provider endpoint and release source asset fallback: {}",
                            repo_slug, primary_err
                        ))?
                } else {
                    return Err(primary_err);
                }
            }
        };

        let extract_root = self.cache_dir.join("extract");
        std::fs::create_dir_all(&extract_root).context(format!(
            "Failed to create extraction root '{}'",
            extract_root.display()
        ))?;

        let extracted_path = compression_handler::decompress(&downloaded, &extract_root)
            .context("Failed to unpack source archive")?;
        let workspace_path = Self::resolve_workspace_root(&extracted_path)?;

        Ok(SourceDownload {
            workspace_path,
            release,
        })
    }

    fn resolve_workspace_root(extracted_path: &Path) -> Result<PathBuf> {
        if Self::is_build_root(extracted_path) {
            return Ok(extracted_path.to_path_buf());
        }

        let mut candidates = Vec::new();
        let entries = std::fs::read_dir(extracted_path).context(format!(
            "Failed to scan extracted source root '{}'",
            extracted_path.display()
        ))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && Self::is_build_root(&path) {
                candidates.push(path);
            }
        }

        match candidates.len() {
            0 => Ok(extracted_path.to_path_buf()),
            1 => Ok(candidates.remove(0)),
            _ => {
                candidates.sort();
                let listed = candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(anyhow!(
                    "Build source root is ambiguous under '{}': found multiple candidate repositories [{}]",
                    extracted_path.display(),
                    listed
                ))
            }
        }
    }

    fn is_build_root(path: &Path) -> bool {
        if path.join("Cargo.toml").is_file() {
            return true;
        }

        std::fs::read_dir(path).ok().is_some_and(|entries| {
            entries.flatten().any(|entry| {
                entry.path().extension().is_some_and(|ext| {
                    let ext = ext.to_string_lossy();
                    ext.eq_ignore_ascii_case("sln") || ext.eq_ignore_ascii_case("csproj")
                })
            })
        })
    }

    fn make_source_archive_asset(
        &self,
        repo_slug: &str,
        provider: &Provider,
        release: &Release,
        base_url: Option<&str>,
    ) -> Result<Asset> {
        let url = match provider {
            Provider::Github => format!(
                "https://api.github.com/repos/{}/tarball/{}",
                repo_slug, release.tag
            ),
            Provider::Gitlab => {
                let base = base_url.unwrap_or("https://gitlab.com");
                let encoded = repo_slug.replace('/', "%2F");
                format!(
                    "{}/api/v4/projects/{}/repository/archive.tar.gz?sha={}",
                    base, encoded, release.tag
                )
            }
            Provider::Gitea => {
                let base = base_url.unwrap_or("https://gitea.com");
                format!(
                    "{}/api/v1/repos/{}/archive/{}.tar.gz",
                    base, repo_slug, release.tag
                )
            }
            Provider::Direct | Provider::WebScraper => {
                return Err(anyhow!(
                    "Build supports forge providers only (github/gitlab/gitea)"
                ));
            }
        };

        let asset_name = format!("{}-{}.tar.gz", repo_slug.replace('/', "-"), release.tag);
        Ok(Asset::new(url, 0, asset_name, 0, Utc::now()))
    }

    fn find_release_source_asset(release: &Release) -> Option<&Asset> {
        release
            .assets
            .iter()
            .find(|asset| asset.name.starts_with("source."))
    }
}

impl<'a> Drop for SourceDownloader<'a> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.cache_dir);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SourceDownloader;

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-downloader-test-{name}-{nanos}"))
    }

    #[test]
    fn resolve_workspace_root_uses_root_when_manifest_exists() {
        let root = temp_root("root-manifest");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write Cargo.toml");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve root");
        assert_eq!(resolved, root);
        let _ = std::fs::remove_dir_all(&resolved);
    }

    #[test]
    fn resolve_workspace_root_selects_single_child_repo() {
        let root = temp_root("single-child");
        let child = root.join("repo");
        std::fs::create_dir_all(&child).expect("create child");
        std::fs::write(
            child.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(root.join("pax_global_header"), "").expect("write pax marker");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve child root");
        assert_eq!(resolved, child);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_workspace_root_errors_on_ambiguous_children() {
        let root = temp_root("ambiguous");
        let a = root.join("repo-a");
        let b = root.join("repo-b");
        std::fs::create_dir_all(&a).expect("create repo-a");
        std::fs::create_dir_all(&b).expect("create repo-b");
        std::fs::write(
            a.join("Cargo.toml"),
            "[package]\nname='a'\nversion='0.1.0'\n",
        )
        .expect("write Cargo.toml a");
        std::fs::write(
            b.join("Cargo.toml"),
            "[package]\nname='b'\nversion='0.1.0'\n",
        )
        .expect("write Cargo.toml b");

        let err = SourceDownloader::resolve_workspace_root(&root).expect_err("must be ambiguous");
        assert!(err.to_string().contains("ambiguous"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_workspace_root_returns_input_when_no_candidates_exist() {
        let root = temp_root("no-candidates");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(root.join("README.md"), "hello").expect("write readme");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve fallback");
        assert_eq!(resolved, root);
        let _ = std::fs::remove_dir_all(&resolved);
    }
}
