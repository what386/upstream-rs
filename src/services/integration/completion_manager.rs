use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::{
    models::{common::enums::Provider, provider::Release},
    providers::provider_manager::ProviderManager,
    utils::{
        filesystem::path_exists_no_follow, platform::shells::installed_shell_commands,
        static_paths::UpstreamPaths,
    },
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

/// Concrete destinations for package completion files.
///
/// This deliberately contains only completion output paths, so a replacement
/// can stage completions without manufacturing an alternate application root.
#[derive(Debug, Clone)]
pub struct CompletionPaths {
    pub bash_dir: PathBuf,
    pub fish_dir: PathBuf,
    pub zsh_dir: PathBuf,
}

impl CompletionPaths {
    pub fn from_upstream(paths: &UpstreamPaths) -> Self {
        Self {
            bash_dir: paths.integration.bash_completions_dir.clone(),
            fish_dir: paths.integration.fish_completions_dir.clone(),
            zsh_dir: paths.integration.zsh_completions_dir.clone(),
        }
    }

    pub fn under(root: &Path) -> Self {
        Self {
            bash_dir: root.join("bash"),
            fish_dir: root.join("fish"),
            zsh_dir: root.join("zsh"),
        }
    }
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

#[derive(Debug, Clone, Default)]
pub struct CompletionSnapshot {
    destinations: Vec<PathBuf>,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl CompletionSnapshot {
    pub fn restore(&self) -> Result<()> {
        for path in &self.destinations {
            if path_exists_no_follow(path)? {
                fs::remove_file(path).with_context(|| {
                    format!(
                        "Failed to remove replacement completion file '{}'",
                        path.display()
                    )
                })?;
            }
        }
        for (path, contents) in &self.files {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create completion directory '{}'",
                        parent.display()
                    )
                })?;
            }
            fs::write(path, contents).with_context(|| {
                format!("Failed to restore completion file '{}'", path.display())
            })?;
        }
        Ok(())
    }
}

pub struct CompletionManager<'a> {
    paths: CompletionPaths,
    _upstream_paths: std::marker::PhantomData<&'a UpstreamPaths>,
}

impl<'a> CompletionManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self {
            paths: CompletionPaths::from_upstream(paths),
            _upstream_paths: std::marker::PhantomData,
        }
    }

    pub fn with_paths(paths: CompletionPaths) -> Self {
        Self {
            paths,
            _upstream_paths: std::marker::PhantomData,
        }
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

        self.remove_installed_completion_files(package_name)?;

        let mut installed = 0_usize;
        for (asset, candidate) in selected_assets {
            let mut no_progress: Option<fn(u64, u64)> = None;
            let downloaded_path = provider_manager
                .download_asset(asset, provider, cache_dir, &mut no_progress)
                .await
                .with_context(|| format!("Failed to download completion asset '{}'", asset.name))?;

            self.install_completion(package_name, candidate.shell, &downloaded_path)
                .with_context(|| {
                    format!(
                        "Failed to install '{}' completion from '{}'",
                        candidate.shell.label(),
                        asset.name
                    )
                })?;
            message!(
                message_callback,
                "Installed {} completion from '{}'",
                candidate.shell.label(),
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

        let candidates: Vec<_> = choose_one_per_shell(find_completion_files(package_name, root))
            .into_iter()
            .filter(|candidate| shell_is_available(candidate.shell))
            .collect();
        if candidates.is_empty() {
            return Ok(0);
        }

        self.remove_installed_completion_files(package_name)?;

        let mut installed = 0_usize;
        for candidate in candidates {
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
            CompletionShell::Bash => &self.paths.bash_dir,
            CompletionShell::Fish => &self.paths.fish_dir,
            CompletionShell::Zsh => &self.paths.zsh_dir,
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

    pub fn package_completion_paths(&self, package_name: &str) -> [PathBuf; 3] {
        [
            self.completion_path(package_name, CompletionShell::Bash),
            self.completion_path(package_name, CompletionShell::Fish),
            self.completion_path(package_name, CompletionShell::Zsh),
        ]
    }

    fn remove_installed_completion_files(&self, package_name: &str) -> Result<usize> {
        let candidates = self.package_completion_paths(package_name);

        let mut removed = 0_usize;
        for path in candidates {
            if !path_exists_no_follow(&path)? {
                continue;
            }
            fs::remove_file(&path).with_context(|| {
                format!("Failed to remove completion file '{}'", path.display())
            })?;
            removed += 1;
        }

        Ok(removed)
    }

    pub fn snapshot_for_package(&self, package_name: &str) -> Result<CompletionSnapshot> {
        let candidates = [
            self.completion_path(package_name, CompletionShell::Bash),
            self.completion_path(package_name, CompletionShell::Fish),
            self.completion_path(package_name, CompletionShell::Zsh),
        ];
        let mut files = Vec::new();
        for path in &candidates {
            if !path_exists_no_follow(path)? {
                continue;
            }
            let contents = fs::read(path).with_context(|| {
                format!("Failed to snapshot completion file '{}'", path.display())
            })?;
            files.push((path.clone(), contents));
        }
        Ok(CompletionSnapshot {
            destinations: candidates.into(),
            files,
        })
    }

    pub fn restore_snapshot(&self, snapshot: &CompletionSnapshot) -> Result<()> {
        snapshot.restore()
    }

    pub fn rename_for_package(&self, old_name: &str, new_name: &str) -> Result<bool> {
        let pairs = [
            (
                self.completion_path(old_name, CompletionShell::Bash),
                self.completion_path(new_name, CompletionShell::Bash),
            ),
            (
                self.completion_path(old_name, CompletionShell::Fish),
                self.completion_path(new_name, CompletionShell::Fish),
            ),
            (
                self.completion_path(old_name, CompletionShell::Zsh),
                self.completion_path(new_name, CompletionShell::Zsh),
            ),
        ];

        for (source, destination) in &pairs {
            if path_exists_no_follow(destination)? {
                return Err(anyhow!(
                    "Refusing to claim existing completion file '{}' while renaming '{}'",
                    destination.display(),
                    source.display()
                ));
            }
        }

        let mut renamed = Vec::new();
        for (source, destination) in &pairs {
            if !path_exists_no_follow(source)? {
                continue;
            }
            if let Err(error) = fs::rename(source, destination) {
                let rollback_error =
                    renamed
                        .iter()
                        .rev()
                        .find_map(|(old_path, new_path): &(PathBuf, PathBuf)| {
                            fs::rename(new_path, old_path).err()
                        });
                return match rollback_error {
                    Some(rollback_error) => Err(anyhow!(
                        "Failed to rename completion '{}' to '{}': {}. Rollback also failed: {}",
                        source.display(),
                        destination.display(),
                        error,
                        rollback_error
                    )),
                    None => Err(error).with_context(|| {
                        format!(
                            "Failed to rename completion '{}' to '{}'",
                            source.display(),
                            destination.display()
                        )
                    }),
                };
            }
            renamed.push((source.clone(), destination.clone()));
        }

        Ok(!renamed.is_empty())
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
            if !path_exists_no_follow(&path)? {
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
    use super::{
        CompletionManager, CompletionShell, choose_one_per_shell, classify_completion_path,
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
    fn installs_completions_from_root_to_shell_directories() {
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

        for shell in CompletionManager::installed_shells() {
            match shell {
                CompletionShell::Bash => assert_eq!(
                    fs::read_to_string(paths.integration.bash_completions_dir.join("rg"))
                        .expect("bash installed"),
                    "complete -F _rg rg\n"
                ),
                CompletionShell::Fish => assert_eq!(
                    fs::read_to_string(paths.integration.fish_completions_dir.join("rg.fish"))
                        .expect("fish installed"),
                    "complete -c rg\n"
                ),
                CompletionShell::Zsh => assert!(
                    !paths.integration.zsh_completions_dir.join("_rg").exists(),
                    "source fixture does not include a zsh completion"
                ),
            }
        }

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn remove_for_package_removes_installed_completion_files() {
        let root = test_support::temp_root("upstream-completion-manager", "remove");
        let paths = test_support::upstream_paths(&root);
        let manager = CompletionManager::new(&paths);

        fs::create_dir_all(&paths.integration.bash_completions_dir).expect("create bash dir");
        fs::create_dir_all(&paths.integration.fish_completions_dir).expect("create fish dir");
        fs::create_dir_all(&paths.integration.zsh_completions_dir).expect("create zsh dir");
        fs::write(
            paths.integration.bash_completions_dir.join("rg"),
            "bash installed\n",
        )
        .expect("write installed bash");
        fs::write(
            paths.integration.fish_completions_dir.join("rg.fish"),
            "fish installed\n",
        )
        .expect("write installed fish");
        fs::write(
            paths.integration.zsh_completions_dir.join("_rg"),
            "zsh installed\n",
        )
        .expect("write installed zsh");

        let mut no_messages: Option<fn(&str)> = None;
        let removed = manager
            .remove_for_package("rg", &mut no_messages)
            .expect("remove completions");

        assert_eq!(removed, 3);
        assert!(!paths.integration.bash_completions_dir.join("rg").exists());
        assert!(
            !paths
                .integration
                .fish_completions_dir
                .join("rg.fish")
                .exists()
        );
        assert!(!paths.integration.zsh_completions_dir.join("_rg").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn completion_snapshot_restores_previous_files_and_removes_replacements() {
        let root = test_support::temp_root("upstream-completion-manager", "snapshot");
        let paths = test_support::upstream_paths(&root);
        let manager = CompletionManager::new(&paths);
        fs::create_dir_all(&paths.integration.bash_completions_dir).expect("create bash dir");
        fs::create_dir_all(&paths.integration.fish_completions_dir).expect("create fish dir");
        let bash = paths.integration.bash_completions_dir.join("tool");
        let fish = paths.integration.fish_completions_dir.join("tool.fish");
        fs::write(&bash, "old bash\n").expect("write old bash");

        let snapshot = manager
            .snapshot_for_package("tool")
            .expect("snapshot completions");
        fs::write(&bash, "new bash\n").expect("replace bash");
        fs::write(&fish, "new fish\n").expect("write replacement-only fish");

        snapshot.restore().expect("restore snapshot");

        assert_eq!(fs::read_to_string(&bash).expect("read bash"), "old bash\n");
        assert!(!fish.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_moves_completion_files_without_overwriting_destination() {
        let root = test_support::temp_root("upstream-completion-manager", "rename");
        let paths = test_support::upstream_paths(&root);
        let manager = CompletionManager::new(&paths);
        fs::create_dir_all(&paths.integration.bash_completions_dir).expect("create bash dir");
        let old = paths.integration.bash_completions_dir.join("old");
        let new = paths.integration.bash_completions_dir.join("new");
        fs::write(&old, "old completion\n").expect("write old");

        assert!(
            manager
                .rename_for_package("old", "new")
                .expect("rename completion")
        );
        assert!(!old.exists());
        assert_eq!(
            fs::read_to_string(&new).expect("read renamed"),
            "old completion\n"
        );

        fs::write(&old, "another old\n").expect("write old again");
        let error = manager
            .rename_for_package("old", "new")
            .expect_err("existing destination must not be overwritten");
        assert!(error.to_string().contains("Refusing to claim"));
        assert_eq!(
            fs::read_to_string(&old).expect("old preserved"),
            "another old\n"
        );
        assert_eq!(
            fs::read_to_string(&new).expect("new preserved"),
            "old completion\n"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
