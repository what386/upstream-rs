use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "macos")]
use anyhow::anyhow;
use anyhow::{Context, Result};

use crate::{
    services::integration::CompletionPaths,
    utils::{filenames::filesystem_name, static_paths::UpstreamPaths},
};

/// Temporary filesystem owned by one package preparation operation.
///
/// Nothing in this workspace is a managed live path.  The workspace is moved
/// into place only by the package activation service after preparation has
/// completed successfully.
pub struct InstallWorkspace {
    root: PathBuf,
    pub appimages_dir: PathBuf,
    pub binaries_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub completions: CompletionPaths,
    pub desktop_dir: PathBuf,
    pub icons_dir: PathBuf,
}

impl InstallWorkspace {
    pub fn new(paths: &UpstreamPaths, package_name: &str) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        let root = paths
            .install
            .tmp_dir
            .join(format!("{}-{nonce}", filesystem_name(package_name)));

        let appimages_dir = root.join("appimages");
        let binaries_dir = root.join("binaries");
        let archives_dir = root.join("archives");
        let completions = CompletionPaths::under(&root.join("completions"));
        let desktop_dir = root.join("desktop");
        let icons_dir = root.join("icons");

        for path in [
            &appimages_dir,
            &binaries_dir,
            &archives_dir,
            &completions.bash_dir,
            &completions.fish_dir,
            &completions.zsh_dir,
            &desktop_dir,
            &icons_dir,
        ] {
            fs::create_dir_all(path).with_context(|| {
                format!(
                    "Failed to create install workspace directory '{}'",
                    path.display()
                )
            })?;
        }

        Ok(Self {
            root,
            appimages_dir,
            binaries_dir,
            archives_dir,
            completions,
            desktop_dir,
            icons_dir,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn completions(&self) -> CompletionPaths {
        self.completions.clone()
    }

    pub fn icons_dir(&self) -> &Path {
        &self.icons_dir
    }

    #[cfg(target_os = "linux")]
    pub fn desktop_path(&self, package_name: &str) -> Result<PathBuf> {
        Ok(self
            .desktop_dir
            .join(format!("{}.desktop", filesystem_name(package_name))))
    }

    #[cfg(target_os = "macos")]
    pub fn desktop_path(&self, _package_name: &str) -> Result<PathBuf> {
        Err(anyhow!("Desktop integration is unsupported on macOS"))
    }

    #[cfg(windows)]
    pub fn desktop_path(&self, package_name: &str) -> Result<PathBuf> {
        Ok(self
            .desktop_dir
            .join(format!("{}.lnk", filesystem_name(package_name))))
    }
}

impl Drop for InstallWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
