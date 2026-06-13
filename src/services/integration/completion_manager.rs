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

    pub fn label(self) -> &'static str {
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

#[derive(Debug, Clone)]
struct CachedCompletion {
    shell: CompletionShell,
    path: PathBuf,
    source: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionCacheMismatchKind {
    Missing,
    Different,
}

#[derive(Debug, Clone)]
pub struct CompletionCacheMismatch {
    pub shell: CompletionShell,
    pub cached_path: PathBuf,
    pub installed_path: PathBuf,
    pub kind: CompletionCacheMismatchKind,
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

        let selected_assets: Vec<_> = [
            CompletionShell::Bash,
            CompletionShell::Fish,
            CompletionShell::Zsh,
        ]
        .into_iter()
        .filter(|shell| shell_is_available(*shell))
        .filter_map(|shell| {
            candidates
                .iter()
                .find(|(_asset, candidate)| candidate.shell == shell)
                .map(|(asset, candidate)| (*asset, candidate.clone()))
        })
        .collect();

        if selected_assets.is_empty() {
            return Ok(0);
        }

        let package_cache_dir = self.prepare_package_cache_dir(package_name)?;
        let mut cached_completions = Vec::new();
        for (asset, candidate) in selected_assets {
            let mut no_progress: Option<fn(u64, u64)> = None;
            let downloaded_path = provider_manager
                .download_asset(asset, provider, cache_dir, &mut no_progress)
                .await
                .with_context(|| format!("Failed to download completion asset '{}'", asset.name))?;

            let cached_completion = self.cache_completion(
                &package_cache_dir,
                package_name,
                candidate.shell,
                &downloaded_path,
                Path::new(&asset.name),
            )?;
            cached_completions.push(cached_completion);
        }

        self.install_cached_completions(package_name, &cached_completions, message_callback)
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

        let candidates = choose_one_per_shell(find_completion_files(package_name, root));
        if candidates.is_empty() {
            return Ok(0);
        }

        let package_cache_dir = self.prepare_package_cache_dir(package_name)?;
        let cached_completions = candidates
            .iter()
            .map(|candidate| {
                self.cache_completion(
                    &package_cache_dir,
                    package_name,
                    candidate.shell,
                    &candidate.path,
                    &candidate.path,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        self.install_cached_completions(package_name, &cached_completions, message_callback)
    }

    pub fn cached_completion_mismatches(
        &self,
        package_name: &str,
    ) -> Result<Vec<CompletionCacheMismatch>> {
        self.cached_completion_mismatches_for_shells(package_name, &Self::installed_shells())
    }

    pub fn copy_cached_completions_to_shells<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        self.copy_cached_completions_to_shells_for_shells(
            package_name,
            &Self::installed_shells(),
            message_callback,
        )
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

    fn prepare_package_cache_dir(&self, package_name: &str) -> Result<PathBuf> {
        let package_cache_dir = self.package_cache_dir(package_name);
        if package_cache_dir.exists() {
            fs::remove_dir_all(&package_cache_dir).with_context(|| {
                format!(
                    "Failed to clear completion cache '{}'",
                    package_cache_dir.display()
                )
            })?;
        }
        fs::create_dir_all(&package_cache_dir).with_context(|| {
            format!(
                "Failed to create completion cache '{}'",
                package_cache_dir.display()
            )
        })?;
        Ok(package_cache_dir)
    }

    fn cache_completion(
        &self,
        package_cache_dir: &Path,
        package_name: &str,
        shell: CompletionShell,
        source: &Path,
        original_source: &Path,
    ) -> Result<CachedCompletion> {
        let cached_path = package_cache_dir.join(cache_completion_file_name(package_name, shell));
        fs::copy(source, &cached_path).with_context(|| {
            format!(
                "Failed to copy completion from '{}' to cache '{}'",
                source.display(),
                cached_path.display()
            )
        })?;
        Ok(CachedCompletion {
            shell,
            path: cached_path,
            source: original_source.to_path_buf(),
        })
    }

    fn install_cached_completions<H>(
        &self,
        package_name: &str,
        cached_completions: &[CachedCompletion],
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        let installable_completions: Vec<_> = cached_completions
            .iter()
            .filter(|cached_completion| shell_is_available(cached_completion.shell))
            .collect();

        if installable_completions.is_empty() {
            return Ok(0);
        }

        self.remove_installed_completion_files(package_name)?;

        let mut installed = 0_usize;
        for cached_completion in installable_completions {
            self.install_completion(
                package_name,
                cached_completion.shell,
                &cached_completion.path,
            )
            .with_context(|| {
                format!(
                    "Failed to install '{}' completion from cache '{}'",
                    cached_completion.shell.label(),
                    cached_completion.path.display()
                )
            })?;
            message!(
                message_callback,
                "Installed {} completion from '{}'",
                cached_completion.shell.label(),
                cached_completion.source.display()
            );
            installed += 1;
        }

        Ok(installed)
    }

    fn cached_completion_mismatches_for_shells(
        &self,
        package_name: &str,
        shells: &[CompletionShell],
    ) -> Result<Vec<CompletionCacheMismatch>> {
        let mut mismatches = Vec::new();
        for shell in shells {
            let cached_path = self.cached_completion_path(package_name, *shell);
            if !cached_path.exists() {
                continue;
            }

            let installed_path = self.completion_path(package_name, *shell);
            if !installed_path.exists() {
                mismatches.push(CompletionCacheMismatch {
                    shell: *shell,
                    cached_path,
                    installed_path,
                    kind: CompletionCacheMismatchKind::Missing,
                });
                continue;
            }

            if !files_have_same_content(&cached_path, &installed_path)? {
                mismatches.push(CompletionCacheMismatch {
                    shell: *shell,
                    cached_path,
                    installed_path,
                    kind: CompletionCacheMismatchKind::Different,
                });
            }
        }

        Ok(mismatches)
    }

    fn copy_cached_completions_to_shells_for_shells<H>(
        &self,
        package_name: &str,
        shells: &[CompletionShell],
        message_callback: &mut Option<H>,
    ) -> Result<usize>
    where
        H: FnMut(&str),
    {
        let mut copied = 0_usize;
        for shell in shells {
            let cached_path = self.cached_completion_path(package_name, *shell);
            if !cached_path.exists() {
                continue;
            }

            self.install_completion(package_name, *shell, &cached_path)
                .with_context(|| {
                    format!(
                        "Failed to copy cached '{}' completion from '{}' to shell directory",
                        shell.label(),
                        cached_path.display()
                    )
                })?;
            message!(
                message_callback,
                "Installed {} completion from '{}'",
                shell.label(),
                cached_path.display()
            );
            copied += 1;
        }

        Ok(copied)
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

    fn package_cache_dir(&self, package_name: &str) -> PathBuf {
        self.paths
            .dirs
            .cache_dir
            .join("completions")
            .join(package_name)
    }

    fn cached_completion_path(&self, package_name: &str, shell: CompletionShell) -> PathBuf {
        self.package_cache_dir(package_name)
            .join(cache_completion_file_name(package_name, shell))
    }

    fn remove_installed_completion_files(&self, package_name: &str) -> Result<usize> {
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
            removed += 1;
        }

        Ok(removed)
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

        let package_cache_dir = self.package_cache_dir(package_name);
        if package_cache_dir.exists() {
            fs::remove_dir_all(&package_cache_dir).with_context(|| {
                format!(
                    "Failed to remove completion cache '{}'",
                    package_cache_dir.display()
                )
            })?;
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

fn cache_completion_file_name(package_name: &str, shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash => format!("{package_name}.bash"),
        CompletionShell::Fish => format!("{package_name}.fish"),
        CompletionShell::Zsh => format!("_{package_name}.zsh"),
    }
}

fn files_have_same_content(left: &Path, right: &Path) -> Result<bool> {
    let left_metadata = fs::metadata(left)
        .with_context(|| format!("Failed to read completion file '{}'", left.display()))?;
    let right_metadata = fs::metadata(right)
        .with_context(|| format!("Failed to read completion file '{}'", right.display()))?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }

    let left_bytes = fs::read(left)
        .with_context(|| format!("Failed to read completion file '{}'", left.display()))?;
    let right_bytes = fs::read(right)
        .with_context(|| format!("Failed to read completion file '{}'", right.display()))?;
    Ok(left_bytes == right_bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionCacheMismatchKind, CompletionManager, CompletionShell,
        cache_completion_file_name, choose_one_per_shell, classify_completion_path,
    };
    use crate::utils::test_support;
    use std::{fs, path::Path};

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

    #[test]
    fn uses_flat_package_completion_cache_filenames() {
        assert_eq!(
            cache_completion_file_name("rg", CompletionShell::Bash),
            "rg.bash"
        );
        assert_eq!(
            cache_completion_file_name("rg", CompletionShell::Fish),
            "rg.fish"
        );
        assert_eq!(
            cache_completion_file_name("rg", CompletionShell::Zsh),
            "_rg.zsh"
        );
    }

    #[test]
    fn caches_completions_from_root_under_package_directory() {
        let root = test_support::temp_root("upstream-completion-manager", "root");
        let paths = test_support::upstream_paths(&root);
        let source_root = root.join("source");
        fs::create_dir_all(source_root.join("completions")).expect("create source");
        fs::write(
            source_root.join("completions/rg.bash"),
            "complete -F _rg rg\n",
        )
        .expect("write bash");
        fs::write(source_root.join("completions/rg.fish"), "complete -c rg\n").expect("write fish");

        let mut no_messages: Option<fn(&str)> = None;
        CompletionManager::new(&paths)
            .install_from_root("rg", &source_root, &mut no_messages)
            .expect("install completions");

        assert_eq!(
            fs::read_to_string(root.join("data/cache/completions/rg/rg.bash")).expect("bash cache"),
            "complete -F _rg rg\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("data/cache/completions/rg/rg.fish")).expect("fish cache"),
            "complete -c rg\n"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reports_cached_completion_mismatches_for_shell_destinations() {
        let root = test_support::temp_root("upstream-completion-manager", "mismatch");
        let paths = test_support::upstream_paths(&root);
        let manager = CompletionManager::new(&paths);

        fs::create_dir_all(root.join("data/cache/completions/rg")).expect("create cache");
        fs::create_dir_all(&paths.integration.fish_completions_dir).expect("create fish dir");
        fs::create_dir_all(&paths.integration.zsh_completions_dir).expect("create zsh dir");
        fs::write(
            root.join("data/cache/completions/rg/rg.bash"),
            "bash cached\n",
        )
        .expect("write cached bash");
        fs::write(
            root.join("data/cache/completions/rg/rg.fish"),
            "fish cached\n",
        )
        .expect("write cached fish");
        fs::write(
            root.join("data/cache/completions/rg/_rg.zsh"),
            "zsh cached\n",
        )
        .expect("write cached zsh");
        fs::write(
            paths.integration.fish_completions_dir.join("rg.fish"),
            "fish installed\n",
        )
        .expect("write installed fish");
        fs::write(
            paths.integration.zsh_completions_dir.join("_rg"),
            "zsh cached\n",
        )
        .expect("write installed zsh");

        let mismatches = manager
            .cached_completion_mismatches_for_shells(
                "rg",
                &[
                    CompletionShell::Bash,
                    CompletionShell::Fish,
                    CompletionShell::Zsh,
                ],
            )
            .expect("mismatches");

        assert_eq!(mismatches.len(), 2);
        assert_eq!(mismatches[0].shell, CompletionShell::Bash);
        assert_eq!(mismatches[0].kind, CompletionCacheMismatchKind::Missing);
        assert_eq!(mismatches[1].shell, CompletionShell::Fish);
        assert_eq!(mismatches[1].kind, CompletionCacheMismatchKind::Different);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn copies_cached_completions_to_shell_destinations() {
        let root = test_support::temp_root("upstream-completion-manager", "copy");
        let paths = test_support::upstream_paths(&root);
        let manager = CompletionManager::new(&paths);

        fs::create_dir_all(root.join("data/cache/completions/rg")).expect("create cache");
        fs::write(
            root.join("data/cache/completions/rg/rg.bash"),
            "bash cached\n",
        )
        .expect("write cached bash");
        fs::write(
            root.join("data/cache/completions/rg/rg.fish"),
            "fish cached\n",
        )
        .expect("write cached fish");

        let mut no_messages: Option<fn(&str)> = None;
        let copied = manager
            .copy_cached_completions_to_shells_for_shells(
                "rg",
                &[CompletionShell::Bash, CompletionShell::Fish],
                &mut no_messages,
            )
            .expect("copy cached completions");

        assert_eq!(copied, 2);
        assert_eq!(
            fs::read_to_string(paths.integration.bash_completions_dir.join("rg"))
                .expect("bash installed"),
            "bash cached\n"
        );
        assert_eq!(
            fs::read_to_string(paths.integration.fish_completions_dir.join("rg.fish"))
                .expect("fish installed"),
            "fish cached\n"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
