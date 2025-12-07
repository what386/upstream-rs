use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::infrastructure::providers::{GithubClient, GithubReleaseDto, GithubAssetDto};


#[derive(Debug, Clone)]
pub struct GithubAdapter {
    client: GithubClient,
}

impl GithubAdapter {
    /// Creates a new instance of the GithubAdapter with the specified GitHub client.
    pub fn new(client: GithubClient) -> Self {
        Self {
            client
        }
    }

    fn convert_asset(&self, dto: GithubAssetDto) -> Asset {
        let created_at = Self::parse_timestamp(&dto.created_at);

        Asset {
            download_url: dto.browser_download_url,
            id: dto.id as u64,
            name: dto.name,
            size: dto.size as u64,
            content_type: dto.content_type.unwrap_or_default(),
            created_at,
        }
    }

    fn convert_release(&self, dto: GithubReleaseDto) -> Release {
        let assets: Vec<Asset> = dto.assets
            .into_iter()
            .map(|a| self.convert_asset(a))
            .collect();

        let version = Self::parse_version(&dto.tag_name);

        Release {
            id: dto.id as u64,
            tag_name: dto.tag_name,
            name: dto.name,
            body: dto.body,
            is_draft: dto.draft,
            is_prerelease: dto.prerelease,
            published_at: Self::parse_timestamp(&dto.published_at),
            assets,
            version,
        }
    }

    /// Downloads an asset to the specified path.
    ///
    /// # Arguments
    /// * `asset` - The asset object containing the download URL
    /// * `destination_path` - The path to download the file into
    /// * `progress_callback` - Optional callback for download progress (downloaded_bytes, total_bytes)
    pub async fn download_asset<F>(
        &self,
        asset: &Asset,
        destination_path: impl AsRef<Path>,
        progress_callback: Option<F>,
    ) -> Result<()>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        self.client
            .download_file(&asset.download_url, destination_path, progress_callback)
            .await
    }

    /// Gets the release for the specified repository matching the given tag.
    ///
    /// # Arguments
    /// * `slug` - The repository in the format "owner/repo"
    /// * `tag` - The tag name to retrieve (e.g., "v1.0.0")
    pub async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        let dto = self.client.get_release_by_tag(slug, tag).await?;
        Ok(self.convert_release(dto))
    }

    /// Gets the release for the specified repository matching the given release ID.
    ///
    /// # Arguments
    /// * `slug` - The repository in the format "owner/repo"
    /// * `release_id` - The unique identifier of the release
    pub async fn get_release_by_id(&self, slug: &str, release_id: i64) -> Result<Release> {
        let dto = self.client.get_release_by_id(slug, release_id).await?;
        Ok(self.convert_release(dto))
    }

    /// Gets the latest release for the specified repository.
    ///
    /// # Arguments
    /// * `slug` - The repository in the format "owner/repo"
    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        let dto = self.client.get_latest_release(slug).await?;
        Ok(self.convert_release(dto))
    }

    /// Gets all releases for the specified repository.
    ///
    /// # Arguments
    /// * `slug` - The repository in the format "owner/repo"
    /// * `per_page` - The number of releases to fetch per API request (default is 30)
    pub async fn get_all_releases(&self, slug: &str, per_page: Option<u32>) -> Result<Vec<Release>> {
        let per_page = per_page.unwrap_or(30);
        let dtos = self.client.get_all_releases(slug, per_page).await?;
        Ok(dtos.into_iter().map(|dto| self.convert_release(dto)).collect())
    }

    fn parse_version(tag: &str) -> Version {
        // Examples:
        // v1.2.3 → 1.2.3
        // release-2.5 → 2.5
        let mut cleaned = tag.trim().trim_start_matches(['v', 'V'].as_ref()).to_string();

        // Remove common prefixes like "release-" or "ver-"
        let prefixes = ["release-", "rel-", "ver-", "version-"];
        for prefix in prefixes {
            if cleaned.to_lowercase().starts_with(prefix) {
                cleaned = cleaned[prefix.len()..].to_string();
                break;
            }
        }

        Version::parse(&cleaned).unwrap_or_else(|_| Version::new(0, 0, 0, false))
    }

    fn parse_timestamp(raw: &str) -> DateTime<Utc> {
        if raw.trim().is_empty() {
            return DateTime::<Utc>::MIN_UTC;
        }

        raw.parse::<DateTime<Utc>>()
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
    }
}
