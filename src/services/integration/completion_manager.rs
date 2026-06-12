use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::{
    models::{common::enums::Provider, provider::Release},
    providers::provider_manager::ProviderManager,
    utils::{platform::shells::installed_shell_commands, static_paths::UpstreamPaths},
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl CompletionShell {
    fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            "zsh" => Some(Self::Zsh),
            _ => None,
        }
    }

    fn from_command(command: &str) -> Option<Self> {
        match command {
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            "zsh" => Some(Self::Zsh),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Zsh => "zsh",
        }
    }
}

#[derive(Debug, Clone)]
struct CompletionCandidate {
    shell: CompletionShell,
    path: PathBuf,
    priority: u8,
}

pub struct CompletionManager<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> CompletionManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub fn installed_shells() -> Vec<CompletionShell> {
        installed_completion_shells()
    }

    pub fn installed_shell_completion_dirs(&self) -> Vec<(&'static str, PathBuf)> {
        Self::installed_shells()
            .into_iter()
            .map(|shell| (shell.label(), self.completion_dir(shell).to_path_buf()))
            .collect()
    }

    pub async fn install_from_release_assets<H>(
        &self,
        package_name: &str,
        release: &Release,
        provider_manager: &ProviderManager,
        provider: &Provider,
        cache_dir: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        let mut candidates: Vec<_> = release
            .assets
            .iter()
            .filter_map(|asset| {
                classify_completion_path(package_name, Path::new(&asset.name))
                    .map(|candidate| (asset, candidate))
            })
            .collect();
        candidates.sort_by(|(asset_a, candidate_a), (asset_b, candidate_b)| {
            candidate_a
                .priority
                .cmp(&candidate_b.priority)
                .then_with(|| asset_a.name.cmp(&asset_b.name))
        });

        let mut installed = 0_usize;
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Fish,
            CompletionShell::Zsh,
        ] {
            if !shell_is_available(shell) {
                continue;
            }

            let Some((asset, _candidate)) = candidates
                .iter()
                .find(|(_asset, candidate)| candidate.shell == shell)
            else {
                continue;
            };

            let mut no_progress: Option<fn(u64, u64)> = None;
            let completion_path = provider_manager
                .download_asset(asset, provider, cache_dir, &mut no_progress)
                .await
                .with_context(|| format!("Failed to download completion asset '{}'", asset.name))?;

            self.install_completion(package_name, shell, &completion_path)
                .with_context(|| format!("Failed to install '{}' completion", shell.label()))?;
            message!(
                message_callback,
                "Installed {} completion from '{}'",
                shell.label(),
                asset.name
            );
            installed += 1;
        }

        Ok(installed)
    }

    pub fn install_from_root<H>(
        &self,
        package_name: &str,
        root: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        if !root.exists() {
            return Ok(0);
        }

        let candidates = find_completion_files(package_name, root);
        let mut installed = 0_usize;
        for candidate in choose_one_per_shell(candidates) {
            if !shell_is_available(candidate.shell) {
                continue;
            }

            self.install_completion(package_name, candidate.shell, &candidate.path)
                .with_context(|| {
                    format!(
                        "Failed to install '{}' completion from '{}'",
                        candidate.shell.label(),
                        candidate.path.display()
                    )
                })?;
            message!(
                message_callback,
                "Installed {} completion from '{}'",
                candidate.shell.label(),
                candidate.path.display()
            );
            installed += 1;
        }

        Ok(installed)
    }

    fn install_completion(
        &self,
        package_name: &str,
        shell: CompletionShell,
        source: &Path,
    ) -> Result<()> {
        let destination = self.completion_path(package_name, shell);

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create completion directory '{}'",
                    parent.display()
                )
            })?;
        }
        fs::copy(source, &destination).with_context(|| {
            format!(
                "Failed to copy completion from '{}' to '{}'",
                source.display(),
                destination.display()
            )
        })?;
        Ok(())
    }

    fn completion_dir(&self, shell: CompletionShell) -> &Path {
        match shell {
            CompletionShell::Bash => &self.paths.integration.bash_completions_dir,
            CompletionShell::Fish => &self.paths.integration.fish_completions_dir,
            CompletionShell::Zsh => &self.paths.integration.zsh_completions_dir,
        }
    }

    fn completion_path(&self, package_name: &str, shell: CompletionShell) -> PathBuf {
        match shell {
            CompletionShell::Bash => self.completion_dir(shell).join(package_name),
            CompletionShell::Fish => self
                .completion_dir(shell)
                .join(format!("{package_name}.fish")),
            CompletionShell::Zsh => self.completion_dir(shell).join(format!("_{package_name}")),
        }
    }

    pub fn remove_for_package<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        let candidates = [
            self.completion_path(package_name, CompletionShell::Bash),
            self.completion_path(package_name, CompletionShell::Fish),
            self.completion_path(package_name, CompletionShell::Zsh),
        ];

        let mut removed = 0_usize;
        for path in candidates {
            if !path.exists() {
                continue;
            }
            fs::remove_file(&path).with_context(|| {
                format!("Failed to remove completion file '{}'", path.display())
            })?;
            message!(message_callback, "Removed completion: {}", path.display());
            removed += 1;
        }

        Ok(removed)
    }
}

fn shell_is_available(shell: CompletionShell) -> bool {
    installed_completion_shells().contains(&shell)
}

fn installed_completion_shells() -> Vec<CompletionShell> {
    installed_shell_commands()
        .into_iter()
        .filter_map(|shell| CompletionShell::from_command(&shell))
        .collect()
}

fn find_completion_files(package_name: &str, root: &Path) -> Vec<CompletionCandidate> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| classify_completion_path(package_name, entry.path()))
        .collect()
}

fn choose_one_per_shell(mut candidates: Vec<CompletionCandidate>) -> Vec<CompletionCandidate> {
    candidates.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| {
                a.path
                    .components()
                    .count()
                    .cmp(&b.path.components().count())
            })
            .then_with(|| a.path.cmp(&b.path))
    });

    let mut selected = Vec::new();
    for shell in [
        CompletionShell::Bash,
        CompletionShell::Fish,
        CompletionShell::Zsh,
    ] {
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.shell == shell)
            .cloned()
        {
            selected.push(candidate);
        }
    }
    selected
}

fn classify_completion_path(package_name: &str, path: &Path) -> Option<CompletionCandidate> {
    let file_name = path.file_name()?.to_string_lossy();
    let extension = path.extension()?.to_string_lossy();
    let shell = CompletionShell::from_extension(&extension)?;
    let lower_file_name = file_name.to_ascii_lowercase();
    let lower_package = package_name.to_ascii_lowercase();

    if lower_file_name == format!("{lower_package}.{extension}") {
        return Some(CompletionCandidate {
            shell,
            path: path.to_path_buf(),
            priority: 0,
        });
    }

    if lower_file_name == format!("completions.{extension}") {
        return Some(CompletionCandidate {
            shell,
            path: path.to_path_buf(),
            priority: 1,
        });
    }

    if path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("completions"))
        .unwrap_or(false)
    {
        return Some(CompletionCandidate {
            shell,
            path: path.to_path_buf(),
            priority: 2,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{CompletionShell, choose_one_per_shell, classify_completion_path};
    use std::path::Path;

    #[test]
    fn classifies_supported_completion_names() {
        assert_eq!(
            classify_completion_path("rg", Path::new("rg.fish"))
                .expect("candidate")
                .shell,
            CompletionShell::Fish
        );
        assert_eq!(
            classify_completion_path("rg", Path::new("completions.bash"))
                .expect("candidate")
                .shell,
            CompletionShell::Bash
        );
        assert_eq!(
            classify_completion_path("rg", Path::new("completions/_rg.zsh"))
                .expect("candidate")
                .shell,
            CompletionShell::Zsh
        );
        assert!(classify_completion_path("rg", Path::new("README.md")).is_none());
    }

    #[test]
    fn chooses_best_candidate_per_shell() {
        let candidates = vec![
            classify_completion_path("rg", Path::new("completions/rg.fish")).expect("candidate"),
            classify_completion_path("rg", Path::new("rg.fish")).expect("candidate"),
        ];

        let selected = choose_one_per_shell(candidates);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].path, Path::new("rg.fish"));
    }

    #[test]
    fn maps_supported_shell_command_names() {
        assert_eq!(
            CompletionShell::from_command("bash"),
            Some(CompletionShell::Bash)
        );
        assert_eq!(
            CompletionShell::from_command("fish"),
            Some(CompletionShell::Fish)
        );
        assert_eq!(CompletionShell::from_command("nu"), None);
    }
}
