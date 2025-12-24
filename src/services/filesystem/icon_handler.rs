use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Result, anyhow};
use tokio::process::Command;

use crate::models::common::enums::Filetype;
use crate::utils::static_paths::UpstreamPaths;

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

    pub async fn add_icon(&self, name: &str, path: &Path, filetype: &Filetype) -> Result<PathBuf> {
        let system_icons = &self.paths.integration.xdg_icons_dir;

        let icon_path = match filetype {
            Filetype::AppImage => {
                let extract_path = self.extract_appimage(name, path).await?;
                Self::search_for_best_icon(&extract_path, name)
                    .or_else(|| Self::search_for_best_icon(system_icons, name))
            }
            Filetype::Archive => Self::search_for_best_icon(path, name)
                .or_else(|| Self::search_for_best_icon(system_icons, name)),
            _ => Self::search_for_best_icon(system_icons, name),
        }
        .ok_or_else(|| anyhow!("Could not find icon"))?;

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

    async fn extract_appimage(&self, name: &str, appimage_path: &Path) -> Result<PathBuf> {
        let extract_path = &self.extract_cache.join(name);
        fs::create_dir_all(extract_path)?;

        let mut process = Command::new(appimage_path)
            .arg("--appimage-extract")
            .current_dir(extract_path) // Set working directory
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // TODO: message callback
        //if let Some(stdout) = process.stdout.take() {
        //    let reader = BufReader::new(stdout);
        //    for line in reader.lines().flatten() {
        //    }
        //}

        process.wait().await?;

        let squashfs_root = extract_path.join("squashfs-root");

        if !squashfs_root.exists() {
            return Err(anyhow!("Error extracting appimage"));
        }

        Ok(squashfs_root)
    }

    fn search_for_best_icon(dir: &Path, name: &str) -> Option<PathBuf> {
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
            return None;
        }

        all_candidates
            .into_iter()
            .max_by_key(|path| Self::score_icon(path, name))
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
                // > 10MB, probably not an icon
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
