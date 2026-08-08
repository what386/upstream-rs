use anyhow::{Context, Result};
use std::path::Path;

use crate::{
    models::{
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{artifact::dotslash_parser, packaging::installer::package_cache_key},
};

/// Resolves which asset from a release should be installed for a package,
/// including DotSlash descriptor resolution before falling back to the
/// provider's normal "best match" heuristic.
pub struct AssetSelector<'a> {
    provider_manager: &'a ProviderManager,
    download_cache: &'a Path,
}

impl<'a> AssetSelector<'a> {
    pub fn new(provider_manager: &'a ProviderManager, download_cache: &'a Path) -> Self {
        Self {
            provider_manager,
            download_cache,
        }
    }

    pub async fn select_asset<H>(
        &self,
        package: &Package,
        release: &Release,
        message_callback: Option<&mut H>,
    ) -> Result<Asset>
    where
        H: FnMut(&str),
    {
        if let Some(descriptor) = dotslash_parser::find_asset(release, package) {
            let cache_key = format!("{}-dotslash", package_cache_key(&package.name));
            let descriptor_cache = self.download_cache.join(cache_key);
            std::fs::create_dir_all(&descriptor_cache).context(format!(
                "Failed to create DotSlash download cache '{}'",
                descriptor_cache.display()
            ))?;
            if let Some(asset) = self
                .resolve_asset(package, descriptor, &descriptor_cache, message_callback)
                .await?
            {
                return Ok(asset);
            }
        }

        self.find_recommended_asset(release, package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))
    }

    fn find_recommended_asset(&self, release: &Release, package: &Package) -> Result<Asset> {
        let mut filtered_release = release.clone();
        filtered_release
            .assets
            .retain(|asset| !dotslash_parser::is_asset(&asset.name, package));

        self.provider_manager
            .find_recommended_asset(&filtered_release, package)
    }

    pub async fn try_resolve_selected_asset<H>(
        &self,
        package: &Package,
        asset: &Asset,
        package_download_cache: &Path,
        message_callback: Option<&mut H>,
    ) -> Result<Option<Asset>>
    where
        H: FnMut(&str),
    {
        dotslash_parser::resolve_selected_asset(
            package,
            asset,
            self.provider_manager,
            package_download_cache,
            message_callback,
        )
        .await
    }

    async fn resolve_asset<H>(
        &self,
        package: &Package,
        asset: &Asset,
        download_cache: &Path,
        message_callback: Option<&mut H>,
    ) -> Result<Option<Asset>>
    where
        H: FnMut(&str),
    {
        dotslash_parser::resolve_asset(
            package,
            asset,
            self.provider_manager,
            download_cache,
            message_callback,
        )
        .await
    }
}
