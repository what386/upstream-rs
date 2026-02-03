use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Result, anyhow};
use serde::de;
use tokio::process::Command;

use crate::models::common::enums::Filetype;
use crate::services::integration::permission_handler;
use crate::utils::static_paths::UpstreamPaths;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct IconManager<'a> {
    paths: &'a UpstreamPaths,
    extract_cache: PathBuf,
}

impl<'a> IconManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Result<Self> {
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
        let extract_cache = temp_path.join("appimage_extract");

        fs::create_dir_all(&extract_cache)?;

        Ok(Self {
            paths,
            extract_cache,
        })
    }

    pub async fn add_icon<H>(
        &self,
        name: &str,
        path: &Path,
        filetype: &Filetype,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {

        let icon_path = match filetype {
            Filetype::AppImage => {
                let extract_path =
                    self.extract_appimage(name, path, message_callback).await?;

                Self::search_for_best_icon(&extract_path, name, message_callback)
                    .or_else(|| Self::search_system_icons(name, message_callback))
            }
            Filetype::Archive => {
                Self::search_for_best_icon(path, name, message_callback)
                    .or_else(|| Self::search_system_icons(name, message_callback))
            }
            _ => Self::search_system_icons(name, message_callback),
        }.ok_or_else(|| anyhow!("Could not find icon"))?;

        self.copy_icon_to_output(&icon_path)
    }

    fn copy_icon_to_output(&self, icon_path: &Path) -> Result<PathBuf> {
        let filename = icon_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid icon path"))?;

        let output_path = self.paths.integration.icons_dir.join(filename);

        fs::copy(icon_path, &output_path)?;

        Ok(output_path)
    }

    async fn extract_appimage<H>(
        &self,
        name: &str,
        appimage_path: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let extract_path = &self.extract_cache.join(name);
        fs::create_dir_all(extract_path)?;

        let temp_appimage = extract_path.join("appimage");
        fs::copy(appimage_path, &temp_appimage)?;

        permission_handler::make_executable(&temp_appimage)?;

        message!(message_callback, "Extracting AppImage...");

        let status = Command::new(&temp_appimage)
            .arg("--appimage-extract")
            .current_dir(extract_path)
            .stdout(File::open("/dev/null")?)
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow!("AppImage extraction failed with status {}", status));
        }

        let squashfs_root = extract_path.join("squashfs-root");

        message!(message_callback, "AppImage Extracted!");

        Ok(squashfs_root)
    }


    fn search_system_icons<H>(
        name: &str,
        message_callback: &mut Option<H>,
    ) -> Option<PathBuf>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Searching system icon themes…");

        let home_dir = std::env::var("HOME").ok()?;

        let icon_dirs = vec![
            PathBuf::from(format!("{}/.local/share/icons", home_dir)),
            PathBuf::from(format!("{}/.icons", home_dir)),
            PathBuf::from("/usr/share/icons"),
            PathBuf::from("/usr/local/share/icons"),
            PathBuf::from("/usr/share/pixmaps"),
            PathBuf::from("/usr/local/share/pixmaps"),
        ];

        let name_lower = name.to_lowercase();
        let extensions = [".svg", ".png", ".xpm", ".ico"];

        // Strategy 1: exact matches
        for dir in &icon_dirs {
            if !dir.exists() {
                continue;
            }

            for ext in extensions {
                let exact_match = dir.join(format!("{}{}", name, ext));
                if exact_match.exists() {
                    message!(
                        message_callback,
                        "Found system icon: {}",
                        exact_match.display()
                    );
                    return Some(exact_match);
                }
            }
        }

        message!(message_callback, "Scanning themed icon directories…");

        let common_subdirs = [
            "hicolor/48x48/apps",
            "hicolor/scalable/apps",
            "hicolor/256x256/apps",
        ];

        for dir in &icon_dirs {
            for subdir in common_subdirs {
                let theme_dir = dir.join(subdir);
                if !theme_dir.exists() {
                    continue;
                }

                for ext in extensions {
                    let icon_path = theme_dir.join(format!("{}{}", name, ext));
                    if icon_path.exists() {
                        message!(
                            message_callback,
                            "Found themed icon: {}",
                            icon_path.display()
                        );
                        return Some(icon_path);
                    }
                }
            }
        }

        message!(message_callback, "Falling back to recursive icon search…");

        let mut all_candidates = Vec::new();

        for dir in icon_dirs {
            if !dir.exists() {
                continue;
            }

            for ext in extensions {
                if let Ok(entries) =
                    glob::glob(&format!("{}/**/*{}*{}", dir.display(), name_lower, ext))
                {
                    all_candidates.extend(entries.flatten().take(50));
                    if all_candidates.len() >= 10 {
                        break;
                    }
                }
            }

            if all_candidates.len() >= 10 {
                break;
            }
        }

        if all_candidates.is_empty() {
            message!(message_callback, "No system icons found");
            return None;
        }

        let best = all_candidates
            .into_iter()
            .max_by_key(|path| Self::score_icon(path, name));

        if let Some(ref path) = best {
            message!(
                message_callback,
                "Selected best system icon: {}",
                path.display()
            );
        }

        best
    }

    fn search_for_best_icon<H>(
        dir: &Path,
        name: &str,
        message_callback: &mut Option<H>,
    ) -> Option<PathBuf>
    where
        H: FnMut(&str),
    {
        message!(
            message_callback,
            "Searching extracted files for icons…"
        );

        let mut all_candidates = Vec::new();

        for ext in [".svg", ".png", ".xpm", ".ico"] {
            let exact_match = dir.join(format!("{}{}", name, ext));
            if exact_match.exists() {
                all_candidates.push(exact_match);
            }

            if let Ok(entries) = glob::glob(&format!("{}/**/*{}*{}", dir.display(), name, ext)) {
                all_candidates.extend(entries.flatten());
            }
        }

        if all_candidates.is_empty() {
            message!(message_callback, "No icons found in extracted files");
            return None;
        }

        let best = all_candidates
            .into_iter()
            .max_by_key(|path| Self::score_icon(path, name));

        if let Some(ref path) = best {
            message!(
                message_callback,
                "Selected extracted icon: {}",
                path.display()
            );
        }

        best
    }

    fn score_icon(path: &Path, app_name: &str) -> i32 {
        let mut score = 0;

        let path_str = path.to_string_lossy().to_lowercase();

        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        if path_str.ends_with(".svg") {
            score += 100; // Vector format, scalable
        } else if path_str.ends_with(".png") {
            score += 70; // Good raster format
        } else if path_str.ends_with(".ico") {
            score += 50; // Icon format but limited
        } else if path_str.ends_with(".xpm") {
            score += 30; // Older format
        }

        if file_stem == app_name.to_lowercase() {
            score += 60;
        }

        if file_stem.contains("icon") {
            score += 50;
        }

        if path_str.contains("icons/")
            || path_str.contains("pixmaps/")
            || path_str.contains(".diricon")
        {
            score += 30;
        }

        if file_stem.contains("screenshot")
            || file_stem.contains("banner")
            || file_stem.contains("splash")
            || file_stem.contains("background")
            || file_stem.contains("preview")
        {
            score -= 30;
        }

        let size_indicators = ["16", "22", "24", "32", "48", "64", "128", "256", "512"];
        for size in size_indicators {
            if file_stem.contains(size) {
                score += 20;
                break;
            }
        }

        if path_str.contains("/hicolor/") || path_str.contains("/theme/") {
            score += 25;
        }

        if let Ok(metadata) = fs::metadata(path) {
            let size = metadata.len();
            if size > 10_000_000 {
                // > 10MB, not an icon
                score -= 100;
            } else if size > 1_000_000 {
                // > 1MB, suspicious
                score -= 50;
            } else if (1024..=500_000).contains(&size) {
                // Reasonable icon size
                score += 10;
            } else if size < 512 {
                // Too small to be useful
                score -= 20;
            }
        }

        score
    }
}

impl<'a> Drop for IconManager<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
    }
}
