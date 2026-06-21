use anyhow::Result;

use crate::{models::upstream::Package, providers::provider_manager::ProviderManager};

use super::fetch::fetch_project_readme;
use super::search::{DocsSearchResult, search_readme};

pub async fn run(
    provider_manager: &ProviderManager,
    package: &Package,
    query: &str,
) -> Result<DocsSearchResult> {
    let readme = fetch_project_readme(provider_manager, package).await?;
    Ok(search_readme(
        &package.name,
        &readme.document_name,
        query,
        &readme.contents,
    ))
}
