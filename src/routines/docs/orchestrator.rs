use anyhow::Result;

use crate::{
    models::upstream::Package, providers::provider_manager::ProviderManager,
    utils::static_paths::UpstreamPaths,
};

use super::fetch::{ProjectReadmeSource, fetch_project_readme};
use super::search::{DocsSearchResult, search_readme};

#[derive(Debug, Clone, PartialEq)]
pub struct DocsRunResult {
    pub search: DocsSearchResult,
    pub readme_source: ProjectReadmeSource,
}

pub async fn run(
    provider_manager: &ProviderManager,
    paths: &UpstreamPaths,
    package: &Package,
    query: &str,
    offline: bool,
) -> Result<DocsRunResult> {
    let readme =
        fetch_project_readme(provider_manager, &paths.dirs.cache_dir, package, offline).await?;
    let search = search_readme(
        &package.name,
        &readme.document_name,
        query,
        &readme.contents,
    );
    Ok(DocsRunResult {
        search,
        readme_source: readme.source,
    })
}
