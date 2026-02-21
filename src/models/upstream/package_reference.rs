use crate::models::{
    common::enums::{Channel, Filetype, Provider},
    upstream::Package,
};
use serde::{Deserialize, Serialize};

/// The bare minimum needed to install a package. Essentially the args to
/// `Package::with_defaults` — no install state, no paths, no version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReference {
    pub name: String,
    pub repo_slug: String,
    pub filetype: Filetype,
    pub channel: Channel,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
}

impl PackageReference {
    pub fn into_package(self) -> Package {
        Package::with_defaults(
            self.name,
            self.repo_slug,
            self.filetype,
            self.match_pattern,
            self.exclude_pattern,
            self.channel,
            self.provider,
            self.base_url,
        )
    }

    pub fn from_package(package: Package) -> Self {
        Self {
            name: package.name,
            repo_slug: package.repo_slug,
            filetype: package.filetype,
            channel: package.channel,
            provider: package.provider,
            base_url: package.base_url,
            match_pattern: package.match_pattern,
            exclude_pattern: package.exclude_pattern,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/models/upstream/package_reference.rs"]
mod tests;
