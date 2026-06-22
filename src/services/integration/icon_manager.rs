#[cfg(target_os = "linux")]
use crate::services::artifact::AppImageExtractor;
use crate::{models::common::enums::Filetype, utils::static_paths::UpstreamPaths};
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct IconManager<'a> {
    paths: &'a UpstreamPaths,
    #[cfg(target_os = "linux")]
    extractor: &'a AppImageExtractor,
}

impl<'a> IconManager<'a> {
    #[cfg(target_os = "linux")]
    pub fn new(paths: &'a UpstreamPaths, extractor: &'a AppImageExtractor) -> Self {
        Self { paths, extractor }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub async fn add_icon<H>(
        &self,
        name: &str,
        path: &Path,
        filetype: &Filetype,
        message_callback: &mut Option<H>,
    ) -> Result<Option<PathBuf>>
    where
        H: FnMut(&str),
    {
        let icon_path = match filetype {
            Filetype::AppImage => {
                #[cfg(target_os = "linux")]
                {
                    let squashfs_root =
                        self.extractor.extract(name, path, message_callback).await?;
                    Self::search_for_best_icon(&squashfs_root, name, message_callback)
                        .or_else(|| Self::search_system_icons(name, message_callback))
                }
                #[cfg(not(target_os = "linux"))]
                {
                    anyhow::bail!("AppImage integration is only supported on Linux hosts");
                }
            }
            Filetype::Archive => Self::search_for_best_icon(path, name, message_callback)
                .or_else(|| Self::search_system_icons(name, message_callback)),
            _ => Self::search_system_icons(name, message_callback),
        };

        let Some(icon_path) = icon_path else {
            message!(
                message_callback,
                "No icon found; using empty Icon field in .desktop file"
            );
            return Ok(None);
        };

        self.copy_icon_to_output(&icon_path).map(Some)
    }

    fn copy_icon_to_output(&self, icon_path: &Path) -> Result<PathBuf> {
        let filename = icon_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid icon path"))?;
        let output_path = self.paths.integration.icons_dir.join(filename);
        fs::copy(icon_path, &output_path)?;
        Ok(output_path)
    }

    fn search_system_icons<H>(name: &str, message_callback: &mut Option<H>) -> Option<PathBuf>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Searching system icon themes …");

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
        // Also try hyphenated variant (e.g. "YouTube Music" -> "youtube-music")
        let name_hyphen = name_lower.replace(' ', "-");
        let name_variants: Vec<&str> = if name_hyphen != name_lower {
            vec![&name_lower, &name_hyphen]
        } else {
            vec![&name_lower]
        };
        let extensions = [".svg", ".png", ".xpm", ".ico"];

        // Strategy 1: exact matches (case-insensitive via lowercased variants)
        for dir in &icon_dirs {
            if !dir.exists() {
                continue;
            }
            for variant in &name_variants {
                for ext in extensions {
                    let exact_match = dir.join(format!("{}{}", variant, ext));
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
        }

        // Strategy 2: themed subdirectories
        message!(message_callback, "Scanning themed icon directories …");
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
                for variant in &name_variants {
                    for ext in extensions {
                        let icon_path = theme_dir.join(format!("{}{}", variant, ext));
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
        }

        // Strategy 3: recursive glob — name-matched + generic fallbacks
        message!(message_callback, "Falling back to recursive icon search …");
        let mut all_candidates = Vec::new();

        for dir in &icon_dirs {
            if !dir.exists() {
                continue;
            }
            // Name-matched globs (case-insensitive via lowercased variants)
            for variant in &name_variants {
                for ext in extensions {
                    if let Ok(entries) =
                        glob::glob(&format!("{}/**/*{}*{}", dir.display(), variant, ext))
                    {
                        all_candidates.extend(entries.flatten().take(50));
                    }
                }
            }
            // Generic fallbacks: anything ending in .svg or .ico
            for ext in [".svg", ".ico"] {
                if let Ok(entries) = glob::glob(&format!("{}/**/*{}", dir.display(), ext)) {
                    all_candidates.extend(entries.flatten().take(20));
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

        all_candidates.dedup();
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
        message!(message_callback, "Searching extracted files for icons …");

        let name_lower = name.to_lowercase();
        // Also try hyphenated variant (e.g. "YouTube Music" -> "youtube-music")
        let name_hyphen = name_lower.replace(' ', "-");
        let name_variants: Vec<&str> = if name_hyphen != name_lower {
            vec![&name_lower, &name_hyphen]
        } else {
            vec![&name_lower]
        };

        let mut all_candidates = Vec::new();

        // Name-matched candidates (case-insensitive via lowercased variants)
        for variant in &name_variants {
            for ext in [".svg", ".png", ".xpm", ".ico"] {
                let exact_match = dir.join(format!("{}{}", variant, ext));
                if exact_match.exists() {
                    all_candidates.push(exact_match);
                }
                if let Ok(entries) =
                    glob::glob(&format!("{}/**/*{}*{}", dir.display(), variant, ext))
                {
                    all_candidates.extend(entries.flatten());
                }
            }
        }

        // Generic fallbacks: anything ending in .svg or .ico
        for ext in [".svg", ".ico"] {
            if let Ok(entries) = glob::glob(&format!("{}/**/*{}", dir.display(), ext)) {
                all_candidates.extend(entries.flatten());
            }
        }

        if all_candidates.is_empty() {
            message!(message_callback, "No icons found in extracted files");
            return None;
        }

        all_candidates.dedup();
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
        let mut score = 0i32;

        let path_str = path.to_string_lossy().to_lowercase();
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let name_lower = app_name.to_lowercase();
        let name_hyphen = name_lower.replace(' ', "-");

        // Format scores
        if path_str.ends_with(".svg") {
            score += 100;
        } else if path_str.ends_with(".png") {
            score += 70;
        } else if path_str.ends_with(".ico") {
            score += 50;
        } else if path_str.ends_with(".xpm") {
            score += 30;
        }

        // Name match (check both spaced and hyphenated variants)
        if file_stem == name_lower || file_stem == name_hyphen {
            score += 60;
        } else if file_stem.contains(&name_lower) || file_stem.contains(&name_hyphen) {
            score += 30;
        }

        // Generic "icon" filename bonus — rewards things like "icon.svg" or "app-icon.png"
        // when no name match is found (fallback path)
        if file_stem == "icon" {
            score += 55; // near-name-match quality for a dedicated icon file
        } else if file_stem.ends_with("-icon")
            || file_stem.ends_with("_icon")
            || file_stem.starts_with("icon-")
            || file_stem.starts_with("icon_")
        {
            score += 40;
        } else if file_stem.contains("icon") {
            score += 20;
        }

        // Location bonuses
        if path_str.contains("icons/")
            || path_str.contains("pixmaps/")
            || path_str.contains(".diricon")
        {
            score += 30;
        }
        if path_str.contains("/hicolor/") || path_str.contains("/theme/") {
            score += 25;
        }

        // Penalise obviously wrong images
        if file_stem.contains("screenshot")
            || file_stem.contains("banner")
            || file_stem.contains("splash")
            || file_stem.contains("background")
            || file_stem.contains("preview")
        {
            score -= 30;
        }

        // Prefer larger (but not huge) raster sizes
        let size_indicators = ["16", "22", "24", "32", "48", "64", "128", "256", "512"];
        for size in size_indicators {
            if path_str.contains(&format!("{}x{}", size, size)) || file_stem.contains(size) {
                score += 20;
                break;
            }
        }

        // File-size sanity
        if let Ok(metadata) = fs::metadata(path) {
            let size = metadata.len();
            if size > 10_000_000 {
                score -= 100;
            } else if size > 1_000_000 {
                score -= 50;
            } else if (1024..=500_000).contains(&size) {
                score += 10;
            } else if size < 512 {
                score -= 20;
            }
        }

        score
    }
}
