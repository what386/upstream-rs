use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use tokio::process::Command;

use crate::services::integration::permission_handler;

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
        fs::create_dir_all(&extract_path)?;

        let temp_appimage = extract_path.join("appimage");
        fs::copy(appimage_path, &temp_appimage)?;
        permission_handler::make_executable(&temp_appimage)?;

        message!(message_callback, "Extracting AppImage…");

        let status = Command::new(&temp_appimage)
            .arg("--appimage-extract")
            .current_dir(&extract_path)
            .stdout(File::open("/dev/null")?)
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow!("AppImage extraction failed"));
        }

        Ok(extract_path.join("squashfs-root"))
    }
}
