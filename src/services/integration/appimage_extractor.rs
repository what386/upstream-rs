use crate::services::integration::permission_handler;
use anyhow::{Result, anyhow};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tokio::process::Command;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct AppImageExtractor {
    extract_cache: PathBuf,
}

impl AppImageExtractor {
    pub fn new() -> Result<Self> {
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
        let extract_cache = temp_path.join("appimage_extract");
        fs::create_dir_all(&extract_cache)?;
        Ok(Self { extract_cache })
    }

    /// Extract an AppImage and return the path to squashfs-root.
    /// Caches by name — calling twice with the same name skips re-extraction.
    pub async fn extract<H>(
        &self,
        name: &str,
        appimage_path: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let extract_path = self.extract_cache.join(name);
        let squashfs_root = extract_path.join("squashfs-root");

        // Already extracted this session — reuse it.
        if squashfs_root.exists() {
            message!(message_callback, "Using cached extraction for '{}'", name);
            return Ok(squashfs_root);
        }

        fs::create_dir_all(&extract_path)?;

        let temp_appimage = extract_path.join("appimage");
        fs::copy(appimage_path, &temp_appimage)?;
        permission_handler::make_executable(&temp_appimage)?;

        message!(message_callback, "Extracting AppImage ...");

        let status = Command::new(&temp_appimage)
            .arg("--appimage-extract")
            .current_dir(&extract_path)
            .stdout(File::open("/dev/null")?)
            .stderr(File::open("/dev/null")?)
            .status()
            .await?;

        // Clean up the copied appimage — we only needed it to run the extract.
        let _ = fs::remove_file(&temp_appimage);

        if !status.success() {
            return Err(anyhow!("AppImage extraction failed with status {}", status));
        }

        if !squashfs_root.exists() {
            return Err(anyhow!("Extraction completed but squashfs-root not found"));
        }

        message!(message_callback, "AppImage extracted");
        Ok(squashfs_root)
    }
}

impl Drop for AppImageExtractor {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
    }
}
