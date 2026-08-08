use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

use crate::{services::integration::CompletionPaths, utils::static_paths::UpstreamPaths};

/// Temporary filesystem owned by one package preparation operation.
///
/// Nothing in this workspace is a managed live path.  The workspace is moved
/// into place only by the package activation service after preparation has
/// completed successfully.
pub(crate) struct InstallWorkspace {
    root: PathBuf,
    pub(crate) appimages_dir: PathBuf,
    pub(crate) binaries_dir: PathBuf,
    pub(crate) archives_dir: PathBuf,
    pub(crate) completions: CompletionPaths,
    pub(crate) desktop_dir: PathBuf,
    pub(crate) icons_dir: PathBuf,
}

impl InstallWorkspace {
    pub(crate) fn new(paths: &UpstreamPaths, package_name: &str) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = paths
            .install
            .tmp_dir
            .join(format!("{package_name}-{nonce}"));
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

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn completions(&self) -> CompletionPaths {
        self.completions.clone()
    }

    pub(crate) fn icons_dir(&self) -> &Path {
        &self.icons_dir
    }

    pub(crate) fn desktop_path(&self, package_name: &str) -> PathBuf {
        #[cfg(target_os = "linux")]
        return self.desktop_dir.join(format!("{package_name}.desktop"));
        #[cfg(target_os = "macos")]
        return self.desktop_dir.join(format!("{package_name}.app"));
        #[cfg(windows)]
        return self.desktop_dir.join(format!("{package_name}.lnk"));
    }
}

impl Drop for InstallWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
