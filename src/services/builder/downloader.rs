use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use indicatif::HumanBytes;

use crate::models::{
    common::enums::{Channel, Provider},
    provider::{Asset, Release},
};
use crate::providers::provider_manager::ProviderManager;
use crate::services::integration::compression_handler;

pub struct SourceDownload {
    pub workspace_path: PathBuf,
    pub release: Release,
    pub branch: Option<String>,
    pub commit: Option<String>,
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

    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_source(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
        branch: Option<&str>,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<SourceDownload> {
        if branch.is_some() && tag.is_some() {
            return Err(anyhow!(
                "Build options --tag and --branch are mutually exclusive"
            ));
        }

        if let Some(branch_name) = branch {
            Self::emit_status(
                status_callback,
                format!("Fetching branch head for '{branch_name}' ..."),
            );
            let head_commit = self
                .provider_manager
                .get_branch_head_sha(repo_slug, provider, branch_name, base_url)
                .await
                .context(format!(
                    "Failed to fetch branch head for '{}' on '{}'",
                    branch_name, repo_slug
                ))?;
            let branch_archive =
                self.make_source_archive_asset(repo_slug, provider, branch_name, base_url)?;

            Self::emit_status(status_callback, "Downloading source archive ...");
            let mut download_progress = Some(|downloaded: u64, total: u64| {
                Self::emit_download_status(status_callback, downloaded, total);
            });
            let downloaded = self
                .provider_manager
                .download_asset(
                    &branch_archive,
                    provider,
                    &self.cache_dir,
                    &mut download_progress,
                )
                .await
                .context(format!(
                    "Failed to download branch source archive '{}' for '{}'",
                    branch_name, repo_slug
                ))?;

            let extract_root = self.cache_dir.join("extract");
            std::fs::create_dir_all(&extract_root).context(format!(
                "Failed to create extraction root '{}'",
                extract_root.display()
            ))?;

            Self::emit_status(status_callback, "Unpacking source archive ...");
            let extracted_path = compression_handler::decompress(&downloaded, &extract_root)
                .context("Failed to unpack source archive")?;
            Self::emit_status(status_callback, "Resolving source workspace ...");
            let workspace_path = Self::resolve_workspace_root(&extracted_path)?;

            let release = Release {
                id: 0,
                tag: branch_name.to_string(),
                name: format!("branch {}", branch_name),
                body: String::new(),
                is_draft: false,
                is_prerelease: false,
                published_at: Utc::now(),
                assets: vec![],
                version: crate::models::common::version::Version::new(0, 0, 0, false),
            };

            return Ok(SourceDownload {
                workspace_path,
                release,
                branch: Some(branch_name.to_string()),
                commit: Some(head_commit),
            });
        }

        let release = if let Some(tag_name) = tag {
            Self::emit_status(
                status_callback,
                format!("Fetching release metadata for '{tag_name}' ..."),
            );
            self.provider_manager
                .get_release_by_tag(repo_slug, tag_name, provider, base_url)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'",
                    tag_name, repo_slug
                ))?
        } else {
            Self::emit_status(status_callback, "Fetching latest release metadata ...");
            self.provider_manager
                .get_latest_release(repo_slug, provider, channel, base_url)
                .await
                .context(format!(
                    "Failed to fetch latest release for '{}'",
                    repo_slug
                ))?
        };

        let primary_archive =
            self.make_source_archive_asset(repo_slug, provider, &release.tag, base_url)?;
        Self::emit_status(status_callback, "Downloading source archive ...");
        let downloaded_primary = {
            let mut download_progress = Some(|downloaded: u64, total: u64| {
                Self::emit_download_status(status_callback, downloaded, total);
            });
            self.provider_manager
                .download_asset(
                    &primary_archive,
                    provider,
                    &self.cache_dir,
                    &mut download_progress,
                )
                .await
                .context(format!(
                    "Failed to download source archive for '{}'",
                    repo_slug
                ))
        };

        let downloaded = match downloaded_primary {
            Ok(path) => path,
            Err(primary_err) => {
                if let Some(fallback) = Self::find_release_source_asset(&release) {
                    Self::emit_status(status_callback, "Trying release source asset fallback ...");
                    let mut download_progress = Some(|downloaded: u64, total: u64| {
                        Self::emit_download_status(status_callback, downloaded, total);
                    });
                    self.provider_manager
                        .download_asset(fallback, provider, &self.cache_dir, &mut download_progress)
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

        Self::emit_status(status_callback, "Unpacking source archive ...");
        let extracted_path = compression_handler::decompress(&downloaded, &extract_root)
            .context("Failed to unpack source archive")?;
        Self::emit_status(status_callback, "Resolving source workspace ...");
        let workspace_path = Self::resolve_workspace_root(&extracted_path)?;

        Ok(SourceDownload {
            workspace_path,
            release,
            branch: None,
            commit: None,
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

    fn emit_status(status_callback: &mut Option<&mut dyn FnMut(&str)>, status: impl AsRef<str>) {
        if let Some(callback) = status_callback.as_deref_mut() {
            callback(status.as_ref());
        }
    }

    fn emit_download_status(
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
        downloaded: u64,
        total: u64,
    ) {
        if total == 0 {
            Self::emit_status(
                status_callback,
                format!("Downloading source archive ... {}", HumanBytes(downloaded)),
            );
        } else {
            Self::emit_status(
                status_callback,
                format!(
                    "Downloading source archive ... {} / {}",
                    HumanBytes(downloaded),
                    HumanBytes(total)
                ),
            );
        }
    }

    fn is_build_root(path: &Path) -> bool {
        if path.join("Cargo.toml").is_file() {
            return true;
        }
        if path.join("go.mod").is_file() {
            return true;
        }
        if path.join("build.zig").is_file() {
            return true;
        }
        if path.join("CMakeLists.txt").is_file() {
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
        git_ref: &str,
        base_url: Option<&str>,
    ) -> Result<Asset> {
        let url = match provider {
            Provider::Github => format!(
                "https://api.github.com/repos/{}/tarball/{}",
                repo_slug, git_ref
            ),
            Provider::Gitlab => {
                let base = base_url.unwrap_or("https://gitlab.com");
                let encoded = repo_slug.replace('/', "%2F");
                format!(
                    "{}/api/v4/projects/{}/repository/archive.tar.gz?sha={}",
                    base, encoded, git_ref
                )
            }
            Provider::Gitea => {
                let base = base_url.unwrap_or("https://gitea.com");
                format!(
                    "{}/api/v1/repos/{}/archive/{}.tar.gz",
                    base, repo_slug, git_ref
                )
            }
            Provider::Direct | Provider::WebScraper => {
                return Err(anyhow!(
                    "Build supports forge providers only (github/gitlab/gitea)"
                ));
            }
        };

        let asset_name = format!("{}-{}.tar.gz", repo_slug.replace('/', "-"), git_ref);
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SourceDownloader;

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-downloader-test-{name}-{nanos}"))
    }

    fn fixture_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(relative)
    }

    fn copy_fixture_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).expect("create fixture destination");
        for entry in fs::read_dir(src).expect("read fixture directory") {
            let entry = entry.expect("read fixture entry");
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_fixture_dir(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).expect("copy fixture file");
            }
        }
    }

    #[test]
    fn resolve_workspace_root_uses_root_when_manifest_exists() {
        let root = fixture_path("builder/workspace-roots/rust-single");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve root");
        assert_eq!(resolved, root);
    }

    #[test]
    fn resolve_workspace_root_selects_single_child_repo() {
        let root = temp_root("single-child");
        copy_fixture_dir(&fixture_path("builder/workspace-roots/pax-noise"), &root);
        let child = root.join("child");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve child root");
        assert_eq!(resolved, child);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_workspace_root_uses_go_root_when_manifest_exists() {
        let root = fixture_path("builder/workspace-roots/go-single");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve root");
        assert_eq!(resolved, root);
    }

    #[test]
    fn resolve_workspace_root_uses_zig_root_when_manifest_exists() {
        let root = fixture_path("builder/workspace-roots/zig-single");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve root");
        assert_eq!(resolved, root);
    }

    #[test]
    fn resolve_workspace_root_uses_cmake_root_when_manifest_exists() {
        let root = fixture_path("builder/workspace-roots/cmake-single");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve root");
        assert_eq!(resolved, root);
    }

    #[test]
    fn resolve_workspace_root_errors_on_ambiguous_children() {
        let root = temp_root("ambiguous");
        copy_fixture_dir(
            &fixture_path("builder/workspace-roots/ambiguous-multi"),
            &root,
        );

        let err = SourceDownloader::resolve_workspace_root(&root).expect_err("must be ambiguous");
        assert!(err.to_string().contains("ambiguous"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_workspace_root_returns_input_when_no_candidates_exist() {
        let root = temp_root("no-candidates");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("README.md"), "hello").expect("write readme");

        let resolved = SourceDownloader::resolve_workspace_root(&root).expect("resolve fallback");
        assert_eq!(resolved, root);
        let _ = fs::remove_dir_all(&resolved);
    }
}
