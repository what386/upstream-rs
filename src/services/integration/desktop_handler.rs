use std::{fs, path::{Path, PathBuf}};
use anyhow::Result;
use crate::{
    models::{
        common::{DesktopEntry, enums::Filetype},
    },
    services::integration::appimage_extractor::AppImageExtractor,
    utils::static_paths::UpstreamPaths,
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct DesktopManager<'a> {
    paths: &'a UpstreamPaths,
    extractor: &'a AppImageExtractor,
}

impl<'a> DesktopManager<'a> {
    pub fn new(paths: &'a UpstreamPaths, extractor: &'a AppImageExtractor) -> Self {
        Self { paths, extractor }
    }

    /// Create a .desktop entry for a package. If the package is an AppImage,
    /// extracts the embedded .desktop file and uses it as a base. Otherwise
    /// builds one from scratch.
    pub async fn create_desktop_entry<H>(
        &self,
        name: &str,
        install_path: &Path,
        exec_path: &Path,
        icon_path: Option<&Path>,
        filetype: &Filetype,
        comment: Option<&str>,
        categories: Option<&str>,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let entry = if *filetype == Filetype::AppImage {
            let squashfs_root = self.extractor.extract(name, install_path, message_callback).await?;
            self.find_and_parse_desktop_file(&squashfs_root, name, message_callback)
                .unwrap_or_default()
        } else {
            DesktopEntry {
                name: Some(name.to_string()),
                comment: comment.map(String::from),
                categories: categories.map(String::from),
                ..DesktopEntry::default()
            }
        };

        let entry = entry.sanitize(exec_path, icon_path);
        self.write_entry(name, &entry)
    }

    /// Remove a .desktop entry by package name. No DesktopManager instance needed.
    pub fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn write_entry(&self, name: &str, entry: &DesktopEntry) -> Result<PathBuf> {
        let out_path = self
            .paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        fs::write(&out_path, entry.to_desktop_file())?;
        Ok(out_path)
    }

    fn find_desktop_entry(&self, name: &str) -> Option<PathBuf> {
        let path = self
            .paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        path.exists().then_some(path)
    }

    /// Search squashfs-root for a .desktop file and parse it.
    /// AppImages typically place it at the root or under usr/share/applications/.
    fn find_and_parse_desktop_file<H>(
        &self,
        squashfs_root: &Path,
        name: &str,
        message_callback: &mut Option<H>,
    ) -> Option<DesktopEntry>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Searching for embedded .desktop file …");

        // Check the two most common locations before falling back to a glob.
        let candidates = [
            squashfs_root.join(format!("{}.desktop", name)),
            squashfs_root.join(format!("usr/share/applications/{}.desktop", name)),
        ];

        for path in &candidates {
            if path.exists() {
                message!(message_callback, "Found .desktop file: {}", path.display());
                return Self::parse_desktop_file(path);
            }
        }

        // Glob for any .desktop file in the tree.
        let pattern = format!("{}/**/*.desktop", squashfs_root.display());
        if let Ok(entries) = glob::glob(&pattern) {
            let mut found: Vec<PathBuf> = entries.flatten().collect();

            // Prefer one whose stem matches the package name.
            found.sort_by_key(|p| {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if stem.eq_ignore_ascii_case(name) { 0 } else { 1 }
            });

            if let Some(path) = found.first() {
                message!(message_callback, "Found .desktop file: {}", path.display());
                return Self::parse_desktop_file(path);
            }
        }

        message!(message_callback, "No .desktop file found in AppImage");
        None
    }

    fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
        let content = fs::read_to_string(path).ok()?;
        let mut entry = DesktopEntry::default();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.starts_with('[') || !trimmed.contains('=') {
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim() {
                "Name"       => entry.name = Some(value),
                "Comment"    => entry.comment = Some(value),
                "Exec"       => entry.exec = Some(value),
                "Icon"       => entry.icon = Some(value),
                "Categories" => entry.categories = Some(value),
                "Terminal"   => entry.terminal = value.eq_ignore_ascii_case("true"),
                _            => {}
            }
        }

        Some(entry)
    }
}
