use anyhow::Result;

use crate::{models::upstream::Package, providers::provider_manager::ProviderManager};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReadme {
    pub document_name: String,
    pub contents: String,
}

pub async fn fetch_project_readme(
    provider_manager: &ProviderManager,
    package: &Package,
) -> Result<ProjectReadme> {
    let contents = provider_manager
        .get_project_readme(
            &package.repo_slug,
            &package.provider,
            package.base_url.as_deref(),
        )
        .await?;

    Ok(ProjectReadme {
        document_name: "README.md".to_string(),
        contents,
    })
}
