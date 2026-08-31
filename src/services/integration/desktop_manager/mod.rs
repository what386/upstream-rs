#[cfg(target_os = "linux")]
mod linux_dm;
#[cfg(windows)]
mod windows_dm;

use crate::{
    models::upstream::Package,
    utils::{
        filesystem::{atomic_ops::write_atomic, path_exists_no_follow},
        static_paths::UpstreamPaths,
    },
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::models::common::enums::Filetype;

#[cfg(target_os = "linux")]
use self::linux_dm::LinuxDesktopHandler;
#[cfg(windows)]
use self::windows_dm::WindowsDesktopHandler;

enum PlatformDesktopHandler {
    #[cfg(target_os = "linux")]
    Linux(LinuxDesktopHandler),
    #[cfg(windows)]
    Windows(WindowsDesktopHandler),
    #[cfg(target_os = "macos")]
    Unsupported,
}

impl PlatformDesktopHandler {
    #[cfg(target_os = "linux")]
    fn linux(handler: LinuxDesktopHandler) -> Self {
        Self::Linux(handler)
    }

    #[cfg(windows)]
    fn windows(handler: WindowsDesktopHandler) -> Self {
        Self::Windows(handler)
    }

    async fn add_icon<H>(
        &self,
        paths: &UpstreamPaths,
        name: &str,
        path: &Path,
        filetype: &Filetype,
        output_dir: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<Option<PathBuf>>
    where
        H: FnMut(&str),
    {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(handler) => {
                handler
                    .add_icon(paths, name, path, filetype, output_dir, message_callback)
                    .await
            }
            #[cfg(windows)]
            Self::Windows(handler) => {
                handler
                    .add_icon(paths, name, path, filetype, output_dir, message_callback)
                    .await
            }
            #[cfg(target_os = "macos")]
            Self::Unsupported => Err(anyhow!("Desktop integration is unsupported on macOS")),
        }
    }

    async fn create_entry<H>(
        &self,
        paths: &UpstreamPaths,
        install_path: &Path,
        filetype: &Filetype,
        entry: crate::models::common::DesktopEntry,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(handler) => {
                handler
                    .create_entry(paths, install_path, filetype, entry, message_callback)
                    .await
            }
            #[cfg(windows)]
            Self::Windows(handler) => {
                handler
                    .create_entry(paths, install_path, filetype, entry, message_callback)
                    .await
            }
            #[cfg(target_os = "macos")]
            Self::Unsupported => Err(anyhow!("Desktop integration is unsupported on macOS")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_staged_entry<H>(
        &self,
        name: &str,
        staged_install_path: &Path,
        staged_exec_path: Option<&Path>,
        final_install_path: &Path,
        final_exec_path: Option<&Path>,
        filetype: &Filetype,
        entry: crate::models::common::DesktopEntry,
        entry_path: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(handler) => {
                handler
                    .create_staged_entry(
                        name,
                        staged_install_path,
                        staged_exec_path,
                        final_install_path,
                        final_exec_path,
                        filetype,
                        entry,
                        entry_path,
                        message_callback,
                    )
                    .await
            }
            #[cfg(windows)]
            Self::Windows(handler) => {
                handler
                    .create_staged_entry(
                        name,
                        staged_install_path,
                        staged_exec_path,
                        final_install_path,
                        final_exec_path,
                        filetype,
                        entry,
                        entry_path,
                        message_callback,
                    )
                    .await
            }
            #[cfg(target_os = "macos")]
            Self::Unsupported => Err(anyhow!("Desktop integration is unsupported on macOS")),
        }
    }

    fn remove_entry(&self, paths: &UpstreamPaths, name: &str) -> Result<()> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(_) => LinuxDesktopHandler::remove_entry(paths, name),
            #[cfg(windows)]
            Self::Windows(_) => WindowsDesktopHandler::remove_entry(paths, name),
            #[cfg(target_os = "macos")]
            Self::Unsupported => Err(anyhow!("Desktop integration is unsupported on macOS")),
        }
    }
}

#[derive(Debug, Clone)]
enum FileSnapshot {
    Missing(PathBuf),
    File { path: PathBuf, contents: Vec<u8> },
    Symlink { path: PathBuf, target: PathBuf },
}

impl FileSnapshot {
    fn capture(path: PathBuf) -> Result<Self> {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::Missing(path));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to inspect '{}'", path.display()));
            }
        };

        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .with_context(|| format!("Failed to snapshot symlink '{}'", path.display()))?;

            return Ok(Self::Symlink { path, target });
        }

        if metadata.is_file() {
            let contents = fs::read(&path)
                .with_context(|| format!("Failed to snapshot file '{}'", path.display()))?;

            return Ok(Self::File { path, contents });
        }

        Err(anyhow!(
            "Cannot snapshot unsupported desktop integration path '{}'",
            path.display()
        ))
    }

    fn path(&self) -> &Path {
        match self {
            Self::Missing(path) | Self::File { path, .. } | Self::Symlink { path, .. } => path,
        }
    }

    fn restore(&self) -> Result<()> {
        let path = self.path();
        if path_exists_no_follow(path)? {
            fs::remove_file(path).with_context(|| {
                format!(
                    "Failed to remove replacement desktop integration '{}'",
                    path.display()
                )
            })?;
        }

        match self {
            Self::Missing(_) => Ok(()),
            Self::File { path, contents } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                write_atomic(path, contents).with_context(|| {
                    format!(
                        "Failed to restore desktop integration file '{}'",
                        path.display()
                    )
                })
            }
            Self::Symlink { path, target } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                create_file_symlink(target, path).with_context(|| {
                    format!(
                        "Failed to restore desktop integration symlink '{}'",
                        path.display()
                    )
                })
            }
        }
    }
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[derive(Debug, Clone)]
pub struct DesktopSnapshot {
    entry: FileSnapshot,
    icon: Option<FileSnapshot>,
}

impl DesktopSnapshot {
    pub fn restore(&self, replacement: Option<&Package>) -> Result<()> {
        if let Some(replacement_icon) = replacement.and_then(|package| package.icon_path.as_ref())
            && self
                .icon
                .as_ref()
                .is_none_or(|snapshot| snapshot.path() != replacement_icon)
            && path_exists_no_follow(replacement_icon)?
        {
            fs::remove_file(replacement_icon).with_context(|| {
                format!(
                    "Failed to remove replacement icon '{}'",
                    replacement_icon.display()
                )
            })?;
        }

        if let Some(icon) = &self.icon {
            icon.restore()?;
        }

        self.entry.restore()
    }
}

pub struct DesktopManager<'a> {
    paths: &'a UpstreamPaths,
    backend: PlatformDesktopHandler,
}

impl<'a> DesktopManager<'a> {
    #[cfg(target_os = "macos")]
    pub fn new(_paths: &'a UpstreamPaths) -> Result<Self> {
        Err(anyhow!("Desktop integration is unsupported on macOS"))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn new(paths: &'a UpstreamPaths) -> Result<Self> {
        #[cfg(target_os = "linux")]
        let backend = PlatformDesktopHandler::linux(LinuxDesktopHandler::new()?);

        #[cfg(windows)]
        let backend = PlatformDesktopHandler::windows(WindowsDesktopHandler::new());

        Ok(Self { paths, backend })
    }

    pub async fn enable_package_entry<H>(
        &self,
        package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .clone()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        let previous_icon_path = package.icon_path.clone();
        let icon_path = self
            .backend
            .add_icon(
                self.paths,
                &package.name,
                &install_path,
                &package.filetype,
                &self.paths.state.icons_dir,
                message_callback,
            )
            .await
            .context(format!("Failed to add icon for '{}'", package.name))?;

        let mut desktop_package = package.clone();
        desktop_package.icon_path = icon_path.clone();
        let desktop_entry = crate::models::common::DesktopEntry::from_package(&desktop_package);

        if let Err(err) = self
            .backend
            .create_entry(
                self.paths,
                &install_path,
                &desktop_package.filetype,
                desktop_entry,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to create desktop entry for '{}'",
                desktop_package.name
            ))
        {
            if let Some(new_icon_path) = icon_path.as_ref()
                && Some(new_icon_path) != previous_icon_path.as_ref()
                && new_icon_path.exists()
            {
                fs::remove_file(new_icon_path).context(format!(
                    "Failed to remove icon file at '{}'",
                    new_icon_path.display()
                ))?;
            }

            return Err(err);
        }

        package.icon_path = icon_path;
        if let Some(previous_icon_path) = previous_icon_path
            && Some(&previous_icon_path) != package.icon_path.as_ref()
            && previous_icon_path.exists()
        {
            fs::remove_file(&previous_icon_path).context(format!(
                "Failed to remove previous icon file at '{}'",
                previous_icon_path.display()
            ))?;
        }

        Ok(())
    }

    pub async fn prepare_package_entry<H>(
        &self,
        staged_package: &Package,
        final_package: &mut Package,
        entry_path: &Path,
        icons_dir: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<Option<PathBuf>>
    where
        H: FnMut(&str),
    {
        let staged_install_path = staged_package.install_path.as_ref().ok_or_else(|| {
            anyhow!(
                "Staged package '{}' has no install path",
                staged_package.name
            )
        })?;

        let final_install_path = final_package.install_path.as_ref().ok_or_else(|| {
            anyhow!(
                "Replacement package '{}' has no install path",
                final_package.name
            )
        })?;

        let staged_icon = self
            .backend
            .add_icon(
                self.paths,
                &final_package.name,
                staged_install_path,
                &final_package.filetype,
                icons_dir,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to prepare icon for '{}'",
                final_package.name
            ))?;

        let final_icon = staged_icon.as_ref().map(|path| {
            self.paths
                .state
                .icons_dir
                .join(path.file_name().expect("staged icon output has a filename"))
        });

        let mut desktop_package = final_package.clone();
        desktop_package.icon_path = final_icon.clone();
        let desktop_entry = crate::models::common::DesktopEntry::from_package(&desktop_package);
        self.backend
            .create_staged_entry(
                &final_package.name,
                staged_install_path,
                staged_package
                    .primary_executable()
                    .map(|executable| executable.path.as_path()),
                final_install_path,
                final_package
                    .primary_executable()
                    .map(|executable| executable.path.as_path()),
                &final_package.filetype,
                desktop_entry,
                entry_path,
                message_callback,
            )
            .await?;

        final_package.icon_path = final_icon;
        Ok(staged_icon)
    }

    pub fn disable_package_entry<H>(
        &self,
        package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        if let Some(callback) = message_callback.as_mut() {
            callback("Removing desktop entry ...");
        }

        self.backend
            .remove_entry(self.paths, &package.name)
            .context(format!(
                "Failed to remove desktop entry for '{}'",
                package.name
            ))?;

        if let Some(icon_path) = package.icon_path.take()
            && icon_path.exists()
        {
            fs::remove_file(&icon_path).context(format!(
                "Failed to remove icon file at '{}'",
                icon_path.display()
            ))?;
            if let Some(callback) = message_callback.as_mut() {
                callback(&format!("Removed stored icon: {}", icon_path.display()));
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn remove_entry(_paths: &UpstreamPaths, _name: &str) -> Result<()> {
        Err(anyhow!("Desktop integration is unsupported on macOS"))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        return LinuxDesktopHandler::remove_entry(paths, name);
        #[cfg(windows)]
        return WindowsDesktopHandler::remove_entry(paths, name);
    }

    pub fn snapshot_for_package(
        paths: &UpstreamPaths,
        package: &Package,
    ) -> Result<DesktopSnapshot> {
        let entry = FileSnapshot::capture(Self::managed_entry_path(paths, &package.name)?)?;
        let icon = package
            .icon_path
            .as_ref()
            .map(|path| FileSnapshot::capture(path.clone()))
            .transpose()?;

        Ok(DesktopSnapshot { entry, icon })
    }

    pub fn rename_entry(paths: &UpstreamPaths, old_name: &str, new_name: &str) -> Result<bool> {
        let source = Self::managed_entry_path(paths, old_name)?;
        let destination = Self::managed_entry_path(paths, new_name)?;

        if path_exists_no_follow(&destination)? {
            return Err(anyhow!(
                "Refusing to overwrite existing desktop entry '{}'",
                destination.display()
            ));
        }

        if !path_exists_no_follow(&source)? {
            return Ok(false);
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::rename(&source, &destination).with_context(|| {
            format!(
                "Failed to rename desktop entry '{}' to '{}'",
                source.display(),
                destination.display()
            )
        })?;
        Ok(true)
    }

    #[cfg(target_os = "macos")]
    pub fn managed_entry_path(_paths: &UpstreamPaths, _name: &str) -> Result<PathBuf> {
        Err(anyhow!("Desktop integration is unsupported on macOS"))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn managed_entry_path(paths: &UpstreamPaths, name: &str) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        return Ok(LinuxDesktopHandler::managed_entry_path(paths, name));
        #[cfg(windows)]
        return Ok(WindowsDesktopHandler::managed_entry_path(paths, name));
    }
}
