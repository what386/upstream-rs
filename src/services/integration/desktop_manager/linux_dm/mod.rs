mod desktop;
mod detection;

use super::super::IconManager;
use crate::services::artifact::AppImageExtractor;
use crate::{
    models::{common::enums::Filetype, upstream::Package},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub(super) struct LinuxDesktopHandler {
    extractor: AppImageExtractor,
}

impl LinuxDesktopHandler {
    pub(super) fn should_auto_integrate(package: &Package) -> bool {
        detection::should_auto_integrate(package)
    }

    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            extractor: AppImageExtractor::new()
                .context("Failed to initialize appimage extractor")?,
        })
    }

    pub(super) async fn add_icon<H>(
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
        IconManager::new(paths, &self.extractor)
            .add_icon_to(name, path, filetype, output_dir, message_callback)
            .await
    }
}
