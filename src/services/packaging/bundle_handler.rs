use crate::{
    models::upstream::Package,
    services::integration::{SymlinkManager, permission_handler},
    utils::{fs_move, static_paths::UpstreamPaths},
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct BundleHandler<'a> {
    paths: &'a UpstreamPaths,
    #[cfg(target_os = "macos")]
    extract_cache: &'a Path,
}

#[cfg(target_os = "macos")]
struct MountedDmg {
    mount_point: PathBuf,
    detached: bool,
}

#[cfg(target_os = "macos")]
impl MountedDmg {
    fn attach(dmg_path: &Path, mount_point: PathBuf) -> Result<Self> {
        fs::create_dir_all(&mount_point).context(format!(
            "Failed to create temporary DMG mountpoint '{}'",
            mount_point.display()
        ))?;

        let output = Command::new("hdiutil")
            .arg("attach")
            .arg(dmg_path)
            .arg("-nobrowse")
            .arg("-readonly")
            .arg("-mountpoint")
            .arg(&mount_point)
            .output()
            .context("Failed to execute 'hdiutil attach'")?;

        if !output.status.success() {
            let _ = fs::remove_dir_all(&mount_point);
            return Err(anyhow!(
                "Failed to mount DMG '{}': {}",
                dmg_path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        Ok(Self {
            mount_point,
            detached: false,
        })
    }

    fn detach(&mut self) -> Result<()> {
        if self.detached {
            return Ok(());
        }

        let output = Command::new("hdiutil")
            .arg("detach")
            .arg(&self.mount_point)
            .output()
            .context("Failed to execute 'hdiutil detach'")?;

        if !output.status.success() {
            let force_output = Command::new("hdiutil")
                .arg("detach")
                .arg("-force")
                .arg(&self.mount_point)
                .output()
                .context("Failed to execute 'hdiutil detach -force'")?;

            if !force_output.status.success() {
                return Err(anyhow!(
                    "Failed to detach DMG mountpoint '{}': {}; force detach failed: {}",
                    self.mount_point.display(),
                    String::from_utf8_lossy(&output.stderr).trim(),
                    String::from_utf8_lossy(&force_output.stderr).trim()
                ));
            }
        }

        self.detached = true;
        let _ = fs::remove_dir_all(&self.mount_point);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Drop for MountedDmg {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

impl<'a> BundleHandler<'a> {
    pub fn new(paths: &'a UpstreamPaths, extract_cache: &'a Path) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = extract_cache;

        Self {
            paths,
            #[cfg(target_os = "macos")]
            extract_cache,
        }
    }

    #[cfg(target_os = "macos")]
    fn package_cache_key(package_name: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let sanitized = package_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        format!("{}-{}", sanitized, timestamp)
    }

    fn is_app_bundle(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
    }

    pub fn find_macos_app_bundle(
        extracted_path: &Path,
        package_name: &str,
    ) -> Result<Option<PathBuf>> {
        let bundles = Self::find_macos_app_bundles(extracted_path)?;
        Ok(Self::select_macos_app_bundle(&bundles, package_name))
    }

    fn find_macos_app_bundles(root: &Path) -> Result<Vec<PathBuf>> {
        if root.is_dir() && Self::is_app_bundle(root) {
            return Ok(vec![root.to_path_buf()]);
        }

        if !root.is_dir() {
            return Ok(Vec::new());
        }

        let mut bundles = Vec::new();
        for entry in WalkDir::new(root).follow_links(false) {
            let entry =
                entry.context(format!("Failed to traverse directory '{}'", root.display()))?;
            let path = entry.path();
            if entry.file_type().is_dir() && Self::is_app_bundle(path) {
                bundles.push(path.to_path_buf());
            }
        }

        let mut top_level_bundles = Vec::new();
        for candidate in &bundles {
            let is_nested = bundles
                .iter()
                .any(|other| other != candidate && candidate.starts_with(other));
            if !is_nested {
                top_level_bundles.push(candidate.clone());
            }
        }

        Ok(top_level_bundles)
    }

    fn select_macos_app_bundle(candidates: &[PathBuf], package_name: &str) -> Option<PathBuf> {
        if candidates.is_empty() {
            return None;
        }

        let package_name_lower = package_name.to_lowercase();
        let mut scored: Vec<(PathBuf, i32, u64)> = candidates
            .iter()
            .cloned()
            .map(|path| {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let name_score = if stem == package_name_lower {
                    2
                } else if stem.contains(&package_name_lower) {
                    1
                } else {
                    0
                };
                let size = Self::directory_size(&path);
                (path, name_score, size)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });

        scored.into_iter().next().map(|entry| entry.0)
    }

    fn directory_size(path: &Path) -> u64 {
        let mut total_size = 0u64;
        for entry in WalkDir::new(path).follow_links(false).into_iter().flatten() {
            if entry.file_type().is_file()
                && let Ok(metadata) = entry.metadata()
            {
                total_size = total_size.saturating_add(metadata.len());
            }
        }
        total_size
    }

    fn find_macos_app_executable(app_bundle_path: &Path, package_name: &str) -> Result<PathBuf> {
        let macos_dir = app_bundle_path.join("Contents").join("MacOS");
        if !macos_dir.is_dir() {
            return Err(anyhow!(
                "Invalid .app bundle '{}': missing Contents/MacOS",
                app_bundle_path.display()
            ));
        }

        let package_name_lower = package_name.to_lowercase();
        let mut executables = Vec::new();
        for entry in fs::read_dir(&macos_dir).context(format!(
            "Failed to read app executable directory '{}'",
            macos_dir.display()
        ))? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_symlink() {
                executables.push(entry.path());
            }
        }

        if executables.is_empty() {
            return Err(anyhow!("No executable found in '{}'", macos_dir.display()));
        }

        executables.sort_by_key(|path| {
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if file_name == package_name_lower {
                0
            } else if file_name.starts_with(&package_name_lower) {
                1
            } else {
                2
            }
        });

        Ok(executables.remove(0))
    }

    #[cfg(target_os = "macos")]
    fn copy_path_recursive(src: &Path, dst: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(src)
            .context(format!("Failed to read metadata for '{}'", src.display()))?;
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            let link_target = fs::read_link(src)
                .context(format!("Failed to read symlink '{}'", src.display()))?;
            Self::copy_symlink(src, dst, &link_target)?;
            return Ok(());
        }

        if metadata.is_file() {
            fs::copy(src, dst).context(format!(
                "Failed to copy file from '{}' to '{}'",
                src.display(),
                dst.display()
            ))?;
            fs::set_permissions(dst, metadata.permissions()).context(format!(
                "Failed to preserve file permissions on '{}'",
                dst.display()
            ))?;
            return Ok(());
        }

        if !metadata.is_dir() {
            return Err(anyhow!(
                "Unsupported file type while copying '{}'",
                src.display()
            ));
        }

        if dst.exists() {
            return Err(anyhow!(
                "Destination already exists while copying '{}'",
                dst.display()
            ));
        }

        fs::create_dir_all(dst)
            .context(format!("Failed to create directory '{}'", dst.display()))?;
        fs::set_permissions(dst, metadata.permissions()).context(format!(
            "Failed to preserve permissions on '{}'",
            dst.display()
        ))?;

        for entry in fs::read_dir(src).context(format!(
            "Failed to read source directory '{}'",
            src.display()
        ))? {
            let entry = entry?;
            let src_child = entry.path();
            let dst_child = dst.join(entry.file_name());
            Self::copy_path_recursive(&src_child, &dst_child)?;
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn remove_path_if_exists(path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(path)
            .context(format!("Failed to read metadata for '{}'", path.display()))?;
        let file_type = metadata.file_type();

        if file_type.is_symlink() || metadata.is_file() {
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        } else if metadata.is_dir() {
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        }

        Ok(())
    }

    fn finalize_macos_app_install<H>(
        &self,
        out_path: PathBuf,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let exec_path = Self::find_macos_app_executable(&out_path, &package.name)?;
        permission_handler::make_executable(&exec_path).context(format!(
            "Failed to make app executable '{}' executable",
            exec_path.display()
        ))?;

        message!(
            message_callback,
            "Using app executable '{}'",
            exec_path.display()
        );

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&exec_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            exec_path.display()
        );

        package.install_path = Some(out_path);
        package.exec_path = Some(exec_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    pub fn install_dmg<H>(
        &self,
        dmg_path: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (dmg_path, package, message_callback);
            Err(anyhow!("DMG installation is only supported on macOS hosts"))
        }

        #[cfg(target_os = "macos")]
        {
            if !dmg_path.exists() || !dmg_path.is_file() {
                return Err(anyhow!(
                    "Invalid DMG path '{}': file not found",
                    dmg_path.display()
                ));
            }

            let mount_point = self.extract_cache.join(format!(
                "dmg-mount-{}",
                Self::package_cache_key(&package.name)
            ));

            message!(
                message_callback,
                "Mounting DMG '{}' ...",
                dmg_path.display()
            );
            let mut mounted = MountedDmg::attach(dmg_path, mount_point)?;

            message!(message_callback, "Searching DMG for .app bundle ...");
            let app_bundles = Self::find_macos_app_bundles(&mounted.mount_point)
                .context("Failed to inspect mounted DMG contents")?;
            let Some(app_bundle_path) = Self::select_macos_app_bundle(&app_bundles, &package.name)
            else {
                return Err(anyhow!(
                    "No .app bundle found in mounted DMG '{}'",
                    dmg_path.display()
                ));
            };

            let bundle_name = app_bundle_path
                .file_name()
                .ok_or_else(|| anyhow!("Invalid .app path: no filename"))?;
            let out_path = self.paths.install.archives_dir.join(bundle_name);

            Self::remove_path_if_exists(&out_path)?;
            message!(
                message_callback,
                "Copying app bundle to '{}' ...",
                out_path.display()
            );
            Self::copy_path_recursive(&app_bundle_path, &out_path).context(format!(
                "Failed to copy app bundle from mounted DMG to '{}'",
                out_path.display()
            ))?;

            mounted.detach()?;

            self.finalize_macos_app_install(out_path, package, message_callback)
        }
    }

    pub fn install_app_bundle<H>(
        &self,
        app_bundle_path: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        if !Self::is_app_bundle(app_bundle_path) || !app_bundle_path.is_dir() {
            return Err(anyhow!(
                "Expected .app bundle directory, got '{}'",
                app_bundle_path.display()
            ));
        }

        let bundle_name = app_bundle_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid .app path: no filename"))?;
        let out_path = self.paths.install.archives_dir.join(bundle_name);

        message!(
            message_callback,
            "Moving app bundle to '{}' ...",
            out_path.display()
        );

        fs_move::move_file_or_dir(app_bundle_path, &out_path).context(format!(
            "Failed to move app bundle to '{}'",
            out_path.display()
        ))?;

        self.finalize_macos_app_install(out_path, package, message_callback)
    }

    #[cfg(target_os = "macos")]
    fn copy_symlink(src: &Path, dst: &Path, link_target: &Path) -> Result<()> {
        let _ = src;
        std::os::unix::fs::symlink(link_target, dst).context(format!(
            "Failed to create symlink '{}' -> '{}'",
            dst.display(),
            link_target.display()
        ))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/services/packaging/bundle_handler.rs"]
mod tests;
