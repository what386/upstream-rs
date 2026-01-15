use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};

use crate::utils::static_paths::UpstreamPaths;

pub struct DesktopManager<'a> {
    paths: &'a UpstreamPaths,
    extract_cache: PathBuf,
}

impl<'a> DesktopManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Result<Self> {
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
        let extract_cache = temp_path.join("appimages");

        fs::create_dir_all(&extract_cache)?;

        Ok(Self {
            paths,
            extract_cache,
        })
    }

    // TODO: extract .desktop info from appimages

    pub fn remove_entry(&self, name: &str) -> Result<()> {
        let path = self
            .find_desktop_entry(name)
            .ok_or_else(|| anyhow!("Could not find icon to create desktop file."))?;

        fs::remove_file(&path)?;

        Ok(())
    }

    pub fn create_desktop_entry(
        &self,
        name: &str,
        exec_path: &Path,
        icon_path: &Path,
        comment: Option<&str>,
        categories: Option<&str>,
    ) -> Result<PathBuf> {
        let mut content = String::from("[Desktop Entry]\nType=Application\nVersion=1.0\n");
        content.push_str(&format!("Name={}\n", name));
        content.push_str(&format!("Exec={}\n", exec_path.display()));
        content.push_str(&format!("Icon={}\n", icon_path.display()));

        if let Some(cmt) = comment {
            content.push_str(&format!("Comment={}\n", cmt));
        }

        content.push_str(&format!(
            "Categories={}\n",
            categories.unwrap_or("Application;")
        ));
        content.push_str("Terminal=false\n");

        let entry = format!("{}.desktop", name);
        let out_path = self.paths.integration.xdg_applications_dir.join(entry);

        fs::write(&out_path, content)?;

        Ok(out_path)
    }

    fn find_desktop_entry(&self, name: &str) -> Option<PathBuf> {
        let appdir = &self.paths.integration.xdg_applications_dir;
        let entry = format!("{}.desktop", name);
        let filepath = appdir.join(entry);

        if filepath.exists() {
            Some(filepath)
        } else {
            None
        }
    }

    fn parse_desktop_file(desktop_file_path: &str) -> HashMap<String, String> {
        let mut metadata = HashMap::new();

        if let Ok(content) = fs::read_to_string(desktop_file_path) {
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || !line.contains('=') {
                    continue;
                }

                if let Some((key, value)) = line.split_once('=') {
                    metadata.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
        metadata
    }
}
