use std::path::{Path, PathBuf};
use std::process::Command;
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
use crate::utils::{filesystem::manifest_sync::sync_manifested_tree, static_paths::UpstreamPaths};

pub struct SourceDownload {
    pub workspace_path: PathBuf,
    pub release: Release,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

pub struct SourceDownloader<'a> {
    provider_manager: &'a ProviderManager,
    cache_dir: PathBuf,
    source_archive_cache_dir: PathBuf,
    archive_cache_dir: PathBuf,
}

impl<'a> SourceDownloader<'a> {
    pub fn new(provider_manager: &'a ProviderManager, paths: &UpstreamPaths) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let cache_dir = paths.dirs.cache_dir.join("build");
        let source_archive_cache_dir = paths.dirs.cache_dir.join("source-archives");
        let archive_cache_dir = std::env::temp_dir().join(format!("upstream-build-{nonce}"));
        std::fs::create_dir_all(&cache_dir).context(format!(
            "Failed to create build cache '{}'",
            cache_dir.display()
        ))?;
        std::fs::create_dir_all(&source_archive_cache_dir).context(format!(
            "Failed to create source archive cache '{}'",
            source_archive_cache_dir.display()
        ))?;
        std::fs::create_dir_all(&archive_cache_dir).context(format!(
            "Failed to create temporary build archive cache '{}'",
            archive_cache_dir.display()
        ))?;

        Ok(Self {
            provider_manager,
            cache_dir,
            source_archive_cache_dir,
            archive_cache_dir,
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

        match self
            .fetch_source_from_git(
                repo_slug,
                provider,
                base_url,
                channel,
                tag,
                branch,
                status_callback,
            )
            .await
        {
            Ok(source) => return Ok(source),
            Err(err) => {
                Self::emit_status(
                    status_callback,
                    format!("Git source cache unavailable, falling back to source archive: {err}"),
                );
            }
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
                    &self.archive_cache_dir,
                    &mut download_progress,
                )
                .await
                .context(format!(
                    "Failed to download branch source archive '{}' for '{}'",
                    branch_name, repo_slug
                ))?;

            let extract_root = self.archive_cache_dir.join("extract");
            std::fs::create_dir_all(&extract_root).context(format!(
                "Failed to create extraction root '{}'",
                extract_root.display()
            ))?;

            Self::emit_status(status_callback, "Unpacking source archive ...");
            let extracted_path = compression_handler::decompress(&downloaded, &extract_root)
                .context("Failed to unpack source archive")?;
            Self::emit_status(status_callback, "Resolving source workspace ...");
            let workspace_path = Self::resolve_workspace_root(&extracted_path)?;
            let cached_workspace = self.cache_archive_workspace(
                repo_slug,
                provider,
                base_url,
                branch_name,
                &workspace_path,
                status_callback,
            )?;

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
                workspace_path: cached_workspace,
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
                .context(format!("fetch latest release for '{}'", repo_slug))?
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
                    &self.archive_cache_dir,
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
                            .download_asset(
                                fallback,
                                provider,
                                &self.archive_cache_dir,
                                &mut download_progress,
                            )
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

        let extract_root = self.archive_cache_dir.join("extract");
        std::fs::create_dir_all(&extract_root).context(format!(
            "Failed to create extraction root '{}'",
            extract_root.display()
        ))?;

        Self::emit_status(status_callback, "Unpacking source archive ...");
        let extracted_path = compression_handler::decompress(&downloaded, &extract_root)
            .context("Failed to unpack source archive")?;
        Self::emit_status(status_callback, "Resolving source workspace ...");
        let workspace_path = Self::resolve_workspace_root(&extracted_path)?;
        let cached_workspace = self.cache_archive_workspace(
            repo_slug,
            provider,
            base_url,
            &release.tag,
            &workspace_path,
            status_callback,
        )?;

        Ok(SourceDownload {
            workspace_path: cached_workspace,
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

    fn cache_archive_workspace(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        git_ref: &str,
        workspace_path: &Path,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let cache_key = source_archive_cache_key(provider, base_url, repo_slug, git_ref);
        let cache_root = self.source_archive_cache_dir.join(cache_key);
        let destination = cache_root.join("workspace");
        let manifest_path = cache_root.join("manifest.json");

        Self::emit_status(
            status_callback,
            format!("Syncing source archive cache '{}'", destination.display()),
        );
        sync_manifested_tree(workspace_path, &destination, &manifest_path).context(format!(
            "Failed to sync source archive cache for '{}' at '{}'",
            repo_slug,
            destination.display()
        ))?;

        Ok(destination)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_source_from_git(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
        branch: Option<&str>,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<SourceDownload> {
        let clone_url = git_clone_url(repo_slug, provider, base_url)?;
        let checkout = self
            .cache_dir
            .join(git_cache_key(provider, base_url, repo_slug));

        Self::emit_status(
            status_callback,
            format!("Using cached git repository '{}'", checkout.display()),
        );
        self.ensure_git_checkout(&clone_url, &checkout, status_callback)?;

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

            self.checkout_branch(&checkout, branch_name, status_callback)?;

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

            let commit = self.current_commit(&checkout).ok().or(Some(head_commit));
            return Ok(SourceDownload {
                workspace_path: Self::resolve_workspace_root(&checkout)?,
                release,
                branch: Some(branch_name.to_string()),
                commit,
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
                .context(format!("fetch latest release for '{}'", repo_slug))?
        };

        self.checkout_tag(&checkout, &release.tag, status_callback)?;

        Ok(SourceDownload {
            workspace_path: Self::resolve_workspace_root(&checkout)?,
            release,
            branch: None,
            commit: self.current_commit(&checkout).ok(),
        })
    }

    fn ensure_git_checkout(
        &self,
        clone_url: &str,
        checkout: &Path,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<()> {
        if checkout.join(".git").is_dir() {
            let existing_remote = self
                .git_output(checkout, ["config", "--get", "remote.origin.url"])
                .unwrap_or_default();
            if existing_remote.trim() != clone_url {
                return Err(anyhow!(
                    "Cached repository '{}' points at '{}' instead of '{}'",
                    checkout.display(),
                    existing_remote.trim(),
                    clone_url
                ));
            }

            Self::emit_status(status_callback, "Fetching git updates ...");
            self.git(checkout, ["fetch", "--tags", "--prune", "origin"])?;
            return Ok(());
        }

        if checkout.exists() {
            return Err(anyhow!(
                "Build cache path '{}' exists but is not a git repository",
                checkout.display()
            ));
        }

        if let Some(parent) = checkout.parent() {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create build cache directory '{}'",
                parent.display()
            ))?;
        }

        Self::emit_status(status_callback, "Cloning git repository ...");
        let checkout_arg = checkout.to_string_lossy().to_string();
        run_git(
            None,
            ["clone", "--recurse-submodules", clone_url, &checkout_arg],
        )
    }

    fn checkout_branch(
        &self,
        checkout: &Path,
        branch: &str,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<()> {
        Self::emit_status(
            status_callback,
            format!("Checking out branch '{branch}' ..."),
        );
        self.reset_tracked_changes(checkout)?;
        self.git(
            checkout,
            ["checkout", "-B", branch, &format!("origin/{branch}")],
        )?;
        Self::emit_status(status_callback, "Pulling branch changes ...");
        self.git(checkout, ["pull", "--ff-only", "origin", branch])?;
        self.update_submodules(checkout, status_callback)
    }

    fn checkout_tag(
        &self,
        checkout: &Path,
        tag: &str,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<()> {
        Self::emit_status(status_callback, format!("Checking out tag '{tag}' ..."));
        self.reset_tracked_changes(checkout)?;
        self.git(checkout, ["checkout", "--detach", tag])?;
        self.update_submodules(checkout, status_callback)
    }

    fn reset_tracked_changes(&self, checkout: &Path) -> Result<()> {
        self.git(checkout, ["reset", "--hard", "HEAD"])
    }

    fn update_submodules(
        &self,
        checkout: &Path,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<()> {
        if !checkout.join(".gitmodules").is_file() {
            return Ok(());
        }

        Self::emit_status(status_callback, "Updating git submodules ...");
        self.git(checkout, ["submodule", "update", "--init", "--recursive"])
    }

    fn current_commit(&self, checkout: &Path) -> Result<String> {
        Ok(self
            .git_output(checkout, ["rev-parse", "HEAD"])?
            .trim()
            .to_string())
    }

    fn git<const N: usize>(&self, checkout: &Path, args: [&str; N]) -> Result<()> {
        run_git(Some(checkout), args)
    }

    fn git_output<const N: usize>(&self, checkout: &Path, args: [&str; N]) -> Result<String> {
        git_output(Some(checkout), args)
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
        let _ = std::fs::remove_dir_all(&self.archive_cache_dir);
    }
}

fn git_clone_url(repo_slug: &str, provider: &Provider, base_url: Option<&str>) -> Result<String> {
    match provider {
        Provider::Github => Ok(format!("https://github.com/{repo_slug}.git")),
        Provider::Gitlab => Ok(format!(
            "{}/{}.git",
            normalize_base_url(base_url.unwrap_or("https://gitlab.com")),
            repo_slug.trim_start_matches('/')
        )),
        Provider::Gitea => Ok(format!(
            "{}/{}.git",
            normalize_base_url(base_url.unwrap_or("https://gitea.com")),
            repo_slug.trim_start_matches('/')
        )),
        Provider::Direct | Provider::WebScraper => Err(anyhow!(
            "Git source cache supports forge providers only (github/gitlab/gitea)"
        )),
    }
}

fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn git_cache_key(provider: &Provider, base_url: Option<&str>, repo_slug: &str) -> String {
    let base = base_url
        .map(normalize_base_url)
        .unwrap_or_else(|| provider.to_string());
    sanitize_path_component(&format!("{base}/{repo_slug}"))
}

fn source_archive_cache_key(
    provider: &Provider,
    base_url: Option<&str>,
    repo_slug: &str,
    git_ref: &str,
) -> String {
    let base = base_url
        .map(normalize_base_url)
        .unwrap_or_else(|| provider.to_string());
    sanitize_path_component(&format!("{base}/{repo_slug}/{git_ref}"))
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn run_git<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<()> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = command.output().context("Failed to execute git")?;
    if output.status.success() {
        return Ok(());
    }

    Err(anyhow!(
        "git failed with status {}: {}{}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim(),
        if output.stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            String::new()
        }
    ))
}

fn git_output<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = command.output().context("Failed to execute git")?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    Err(anyhow!(
        "git failed with status {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{SourceDownloader, git_cache_key, git_clone_url, source_archive_cache_key};
    use crate::models::common::enums::Provider;

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
    fn git_clone_url_uses_provider_defaults_and_base_urls() {
        assert_eq!(
            git_clone_url("owner/repo", &Provider::Github, None).expect("github url"),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            git_clone_url(
                "group/repo",
                &Provider::Gitlab,
                Some("https://gitlab.example.com/")
            )
            .expect("gitlab url"),
            "https://gitlab.example.com/group/repo.git"
        );
        assert_eq!(
            git_clone_url("forge/repo", &Provider::Gitea, Some("codeberg.org")).expect("gitea url"),
            "https://codeberg.org/forge/repo.git"
        );
    }

    #[test]
    fn git_cache_key_is_filesystem_safe_and_provider_specific() {
        assert_eq!(
            git_cache_key(&Provider::Github, None, "owner/repo"),
            "github_owner_repo"
        );
        assert_eq!(
            git_cache_key(
                &Provider::Gitlab,
                Some("https://gitlab.example.com"),
                "group/repo"
            ),
            "https___gitlab.example.com_group_repo"
        );
    }

    #[test]
    fn source_archive_cache_key_includes_ref() {
        assert_eq!(
            source_archive_cache_key(&Provider::Github, None, "owner/repo", "v1.2.3"),
            "github_owner_repo_v1.2.3"
        );
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
