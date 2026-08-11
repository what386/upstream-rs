use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use crate::application::cancellation;
use crate::models::common::enums::{Channel, Provider};
use crate::models::common::version::Version;
use crate::models::provider::Release;

use super::downloader::{SourceDownload, SourceDownloader, cache_key};

impl<'a> SourceDownloader<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fetch_source_from_git(
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
            .join(cache_key(base_url, provider, repo_slug));
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
            let release = branch_release(branch_name);
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
            return self.git(checkout, ["fetch", "--tags", "--prune", "origin"]);
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
}

fn branch_release(branch: &str) -> Release {
    Release {
        id: 0,
        tag: branch.to_string(),
        name: format!("branch {branch}"),
        body: String::new(),
        is_draft: false,
        is_prerelease: false,
        published_at: Utc::now(),
        assets: vec![],
        version: Version::new(0, 0, 0, false),
    }
}

fn git_clone_url(repo_slug: &str, provider: &Provider, base_url: Option<&str>) -> Result<String> {
    match provider {
        Provider::Github => Ok(format!("https://github.com/{repo_slug}.git")),
        Provider::Gitlab | Provider::Gitea => Ok(format!(
            "{}/{}.git",
            normalize_base_url(base_url.unwrap_or(if matches!(provider, Provider::Gitlab) {
                "https://gitlab.com"
            } else {
                "https://gitea.com"
            })),
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

fn run_git<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<()> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = run_command_output(&mut command).context("Failed to execute git")?;
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
    let output = run_command_output(&mut command).context("Failed to execute git")?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Err(anyhow!(
        "git failed with status {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn run_command_output(command: &mut Command) -> Result<std::process::Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("Failed to execute command")?;
    loop {
        if cancellation::is_requested() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(cancellation::Cancelled.into());
        }
        if let Some(status) = child.try_wait()? {
            let output = child
                .wait_with_output()
                .context("Failed to collect command output")?;
            debug_assert_eq!(output.status, status);
            return Ok(output);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_key, git_clone_url};
    use crate::models::common::enums::Provider;

    #[test]
    fn git_clone_url_uses_provider_defaults_and_base_urls() {
        assert_eq!(
            git_clone_url("owner/repo", &Provider::Github, None).expect("github"),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            git_clone_url(
                "group/repo",
                &Provider::Gitlab,
                Some("https://gitlab.example.com/")
            )
            .expect("gitlab"),
            "https://gitlab.example.com/group/repo.git"
        );
        assert_eq!(
            git_clone_url("forge/repo", &Provider::Gitea, Some("codeberg.org")).expect("gitea"),
            "https://codeberg.org/forge/repo.git"
        );
    }

    #[test]
    fn git_cache_key_is_filesystem_safe_and_provider_specific() {
        assert_eq!(
            cache_key(None, &Provider::Github, "owner/repo"),
            "github_owner_repo"
        );
        assert_eq!(
            cache_key(
                Some("https://gitlab.example.com"),
                &Provider::Gitlab,
                "group/repo"
            ),
            "https___gitlab.example.com_group_repo"
        );
    }

    #[test]
    fn source_archive_cache_key_includes_ref() {
        assert_eq!(
            cache_key(None, &Provider::Github, "owner/repo/v1.2.3"),
            "github_owner_repo_v1.2.3"
        );
    }
}
