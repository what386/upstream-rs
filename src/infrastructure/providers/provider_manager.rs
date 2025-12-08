use std::path::{Path, PathBuf};
use std::fs;

use crate::utils::platform_info::{ArchitectureInfo, CpuArch, format_arch, format_os};
use crate::models::common::enums::{Filetype, Channel, Provider};
use crate::models::upstream::Package;
use crate::models::provider::{Asset, Release, asset};
use crate::infrastructure::providers::github::{GithubClient, GithubAdapter};

use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub struct Credentials {
    pub github_token: Option<String>,
}

impl Credentials {
    pub fn new(github_token: Option<String>) -> Self {
        Self { github_token }
    }
}

pub struct ProviderManager {
    github: GithubAdapter,
    cache_path: PathBuf,
    architecture_info: ArchitectureInfo,
}

impl ProviderManager {
    pub fn new(credentials: Credentials) -> Result<Self> {
        let architecture_info = ArchitectureInfo::new();
        let github_client = GithubClient::new(credentials.github_token.as_deref());
        let github = GithubAdapter::new(github_client?);
        let cache_path = std::env::temp_dir().join("upstream_downloads");
        fs::create_dir_all(&cache_path)?;
        Ok(Self {
            github,
            cache_path,
            architecture_info,
        })
    }

    pub async fn get_latest_release(
        &self,
        slug: &str,
        provider: &Provider,
    ) -> Result<Release> {
        match provider {
            Provider::Github => self.github.get_latest_release(slug).await,
        }
    }

    pub async fn get_all_releases(
        &self,
        slug: &str,
        provider: Provider,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        match provider {
            Provider::Github => self.github.get_all_releases(slug, per_page).await,
        }
    }

    pub async fn get_release_by_tag(
        &self,
        slug: &str,
        tag: &str,
        provider: &Provider,
    ) -> Result<Release> {
        match provider {
            Provider::Github => self.github.get_release_by_tag(slug, tag).await,
        }
    }

    pub async fn download_asset(
        &self,
        asset: &Asset,
        provider: &Provider,
        progress: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<PathBuf> {
        let file_name = Path::new(&asset.name)
            .file_name()
            .ok_or_else(|| anyhow!("Invalid asset name: {}", asset.name))?;

        let download_filepath = self.cache_path.join(file_name);

        match provider {
            Provider::Github => {
                self.github
                    .download_asset(asset, &download_filepath, progress)
                    .await?;
            }
        }

        Ok(download_filepath)
    }

    pub async fn download_recommended_asset(
        &self,
        release: &Release,
        package: &Package,
        progress: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<PathBuf> {
        let asset = self.find_recommended_asset(release, package)?;
        let path = self.download_asset(&asset, &package.provider, progress).await?;
        Ok(path)
    }

    pub async fn check_for_update(
        &self,
        package: &Package,
    ) -> Result<Option<Release>> {
        if package.is_paused {
            return Ok(None);
        }

        let release = self
            .get_latest_release(&package.repo_slug, &package.provider)
            .await?;

        if Self::is_valid_update(package, &release) {
            Ok(Some(release))
        } else {
            Ok(None)
        }
    }

    fn is_valid_update(package: &Package, release: &Release) -> bool {
        if package.is_paused {
            return false;
        }

        let consider_release = match package.channel {
            Channel::Stable => !release.is_draft && !release.is_prerelease,
            Channel::Beta | Channel::Nightly => !release.is_draft,
            Channel::All => true,
        };

        consider_release && release.version.is_newer_than(&package.version)
    }

    fn is_potentially_compatible(&self, asset: &Asset) -> bool {
        // OS check
        if let Some(target_os) = &asset.target_os {
            if *target_os != self.architecture_info.os_kind {
                return false;
            }
        }

        // Architecture check
        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                return true;
            }

            // Compatibility fallbacks
            if self.architecture_info.cpu_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86 {
                return true;
            }

            if self.architecture_info.cpu_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm {
                return true;
            }

            return *target_arch == self.architecture_info.cpu_arch;
        }

        true
    }

    fn score_asset(&self, asset: &Asset, package: &Package) -> i32 {
        let name = asset.name.to_lowercase();
        let mut score = 0;

        // Architecture match bonus
        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                score += 80;
            } else if self.architecture_info.cpu_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86 {
                score += 30;
            } else if self.architecture_info.cpu_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm {
                score += 30;
            }
        }

        // Archive format preference
        if asset.filetype == Filetype::Archive {
            if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                score += 10;
            } else if name.ends_with(".zip") {
                score += 5;
            }
        }

        // Compression format preference
        if asset.filetype == Filetype::Compressed {
            if name.ends_with(".br") {
                score += 10;
            } else if name.ends_with(".gz") {
                score += 5;
            }
        }

        // Package name match
        if !name.contains(&package.name.to_lowercase()) {
            score -= 40;
        }

        // Very small files
        if asset.size < 100_000 {
            score -= 20;
        }

        score
    }

    fn find_recommended_asset(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Asset> {
        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .collect();

        compatible_assets
            .into_iter()
            .max_by_key(|a| self.score_asset(a, package))
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "No compatible assets found for {} on {}",
                    format_arch(&self.architecture_info.cpu_arch),
                    format_os(&self.architecture_info.os_kind)
                )
            })
    }

    pub fn get_architecture_info(&self) -> &ArchitectureInfo {
        &self.architecture_info
    }
}

